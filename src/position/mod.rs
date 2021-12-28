mod board;
mod castling;
mod fen;
mod piece;
mod square;
mod state;

use crate::bb::Bitboard;
use crate::masks::{CASTLING_PATH, CASTLING_ROOK_START, FILE_BB, RANK_BB};
use crate::mov::{Move, SpecialMove};
use crate::movegen::{bishop_moves, rook_moves};
use crate::precalc::boards::{aligned, between_bb, king_moves, knight_moves, pawn_attacks_from};

pub use board::Board;
pub use castling::{CastleType, CastlingRights};
pub use piece::{Piece, PieceType, PROMO_PIECES};
pub use square::Square;
pub use state::State;

use std::fmt;
use std::ops::Not;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Player {
    White = 0,
    Black = 1,
}

impl Player {
    /// Returns the other player.
    pub fn other_player(&self) -> Self {
        match self {
            Player::White => Player::Black,
            Player::Black => Player::White,
        }
    }

    /// Returns the relative square from a given square.
    #[inline(always)]
    pub fn relative_square(self, sq: Square) -> Square {
        assert!(sq.is_okay());
        sq ^ Square((self) as u8 * 56)
    }

    /// Returns the offset for a single move pawn push.
    #[inline(always)]
    pub fn pawn_push(self) -> i8 {
        match self {
            Player::White => 8,
            Player::Black => -8,
        }
    }
}

impl Not for Player {
    type Output = Self;
    fn not(self) -> Self::Output {
        self.other_player()
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Player::White => write!(f, "White"),
            Player::Black => write!(f, "Black"),
        }
    }
}

#[derive(Clone)]
pub struct Position {
    // Array of pieces
    pub(crate) board: Board,

    // Bitboards for each piece type
    // TODO: should we switch to a scheme where the bitboards give all of each piece type
    // (i.e. white pawns and black pawns are all on one bitboard), and then we have a
    // white_pieces bb and black_pieces bb maintained separately? To get white_pawns, you would
    // do (pawns & white_pieces)
    pub(crate) no_piece: Bitboard,
    pub(crate) white_pawns: Bitboard,
    pub(crate) white_knights: Bitboard,
    pub(crate) white_bishops: Bitboard,
    pub(crate) white_rooks: Bitboard,
    pub(crate) white_queens: Bitboard,
    pub(crate) white_king: Bitboard,
    pub(crate) black_pawns: Bitboard,
    pub(crate) black_knights: Bitboard,
    pub(crate) black_bishops: Bitboard,
    pub(crate) black_rooks: Bitboard,
    pub(crate) black_queens: Bitboard,
    pub(crate) black_king: Bitboard,
    // Bitboards for each color
    pub(crate) white_pieces: Bitboard,
    pub(crate) black_pieces: Bitboard,

    // Piece counts
    pub(crate) white_piece_count: u8,
    pub(crate) black_piece_count: u8,

    // "Invisible" state
    turn: Player,
    pub(crate) castling_rights: CastlingRights,
    pub(crate) ep_square: Option<Square>,
    pub(crate) half_move_clock: u32,
    pub(crate) move_number: u32,

    // `State` struct stores other useful information for fast access
    pub(crate) state: Option<State>,
}

impl Position {
    /// Sets the `State` struct for the current position. Should only be called
    /// when initialising a new `Position`.
    pub fn set_state(&mut self) {
        self.state = Some(State::from_position(&self));
    }

    // CHECKING

    /// Returns a `Bitboard` of possible attacks to a square with a given occupancy.
    /// Includes pieces from both players.
    // TODO: is there any need to pass `occupied` here? Isn't it already available on `self`?
    pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
        (Bitboard(pawn_attacks_from(sq, Player::Black))
            & self.piece_bb(Player::White, PieceType::Pawn))
            | (Bitboard(pawn_attacks_from(sq, Player::White)))
                & self.piece_bb(Player::Black, PieceType::Pawn)
            | (knight_moves(sq) & self.piece_bb_both_players(PieceType::Knight))
            | (rook_moves(occupied, sq)
                & (self.white_rooks | self.black_rooks | self.white_queens | self.black_queens))
            | (bishop_moves(occupied, sq)
                & (self.white_bishops | self.black_bishops | self.white_queens | self.black_queens))
            | (king_moves(sq) & (self.white_king | self.black_king))
    }

    #[inline]
    pub fn turn(&self) -> Player {
        self.turn
    }

    #[inline]
    pub fn occupied(&self) -> Bitboard {
        !self.no_piece
    }

    #[inline]
    pub fn get_occupied_player(&self, player: Player) -> Bitboard {
        match player {
            Player::White => self.white_pieces,
            Player::Black => self.black_pieces,
        }
    }

    #[inline]
    pub fn occupied_white(&self) -> Bitboard {
        self.white_pieces
    }

    #[inline]
    pub fn occupied_black(&self) -> Bitboard {
        self.black_pieces
    }

    /// Outputs the blockers and pinners of a given square in a tuple `(blockers, pinners)`.
    pub fn slider_blockers(&self, sliders: Bitboard, sq: Square) -> (Bitboard, Bitboard) {
        let mut blockers = Bitboard(0);
        let mut pinners = Bitboard(0);
        let occupied = self.occupied();

        let attackers = sliders
            & ((rook_moves(Bitboard(0), sq)
                & (self.piece_two_bb_both_players(PieceType::Rook, PieceType::Queen)))
                | (bishop_moves(Bitboard(0), sq)
                    & (self.piece_two_bb_both_players(PieceType::Bishop, PieceType::Queen))));

        let player_at = self.board.piece_at_sq(sq).player();
        let other_occ = self.get_occupied_player(player_at);
        for attacker_sq in attackers {
            let bb = Bitboard(between_bb(sq, attacker_sq)) & occupied;
            if bb.is_not_empty() && !bb.more_than_one() {
                blockers |= bb;
                if (bb & other_occ).is_not_empty() {
                    pinners |= attacker_sq.to_bb();
                }
            }
        }

        (blockers, pinners)
    }

    #[inline]
    pub fn piece_bb(&self, player: Player, piece_type: PieceType) -> Bitboard {
        match player {
            Player::White => match piece_type {
                PieceType::None => Bitboard::ALL,
                PieceType::Pawn => self.white_pawns,
                PieceType::Knight => self.white_knights,
                PieceType::Bishop => self.white_bishops,
                PieceType::Rook => self.white_rooks,
                PieceType::Queen => self.white_queens,
                PieceType::King => self.white_king,
            },
            Player::Black => match piece_type {
                PieceType::None => Bitboard::ALL,
                PieceType::Pawn => self.black_pawns,
                PieceType::Knight => self.black_knights,
                PieceType::Bishop => self.black_bishops,
                PieceType::Rook => self.black_rooks,
                PieceType::Queen => self.black_queens,
                PieceType::King => self.black_king,
            },
        }
    }
    /// Returns the Bitboard of the Queens and Rooks for a given player.
    #[inline(always)]
    pub fn sliding_piece_bb(&self, player: Player) -> Bitboard {
        self.piece_two_bb(PieceType::Queen, PieceType::Rook, player)
    }
    /// Returns the Bitboard of the Queens and Bishops for a given player.
    #[inline(always)]
    pub fn diagonal_piece_bb(&self, player: Player) -> Bitboard {
        self.piece_two_bb(PieceType::Queen, PieceType::Bishop, player)
    }

    /// Returns the combined BitBoard of both players for a given piece.
    ///
    /// # Examples
    ///
    /// ```
    /// use pleco::{Board,PieceType};
    ///
    /// let chessboard = Board::start_pos();
    /// assert_eq!(chessboard.piece_bb_both_players(PieceType::P).0, 0x00FF00000000FF00);
    /// ```
    /// Returns the combined Bitboard of both players for a given piece.
    #[inline(always)]
    pub fn piece_bb_both_players(&self, piece: PieceType) -> Bitboard {
        match piece {
            PieceType::None => Bitboard(0),
            PieceType::Pawn => self.white_pawns | self.black_pawns,
            PieceType::Knight => self.white_knights | self.black_knights,
            PieceType::Bishop => self.white_bishops | self.black_bishops,
            PieceType::Rook => self.white_rooks | self.black_rooks,
            PieceType::Queen => self.white_queens | self.black_queens,
            PieceType::King => self.white_king | self.black_king,
        }
    }

    #[inline]
    pub fn piece_two_bb(
        &self,
        piece_type_1: PieceType,
        piece_type_2: PieceType,
        player: Player,
    ) -> Bitboard {
        self.piece_bb(player, piece_type_1) | self.piece_bb(player, piece_type_2)
    }

    #[inline]
    pub fn piece_two_bb_both_players(
        &self,
        piece_type_1: PieceType,
        piece_type_2: PieceType,
    ) -> Bitboard {
        self.piece_bb_both_players(piece_type_1) | self.piece_bb_both_players(piece_type_2)
    }
    /// Returns the `Piece` at the given `Square`
    #[inline]
    pub fn piece_at_sq(&self, sq: Square) -> Piece {
        self.board.piece_at_sq(sq)
    }

    /// Return the en passant square for the current position (usually `None` except
    /// after a double pawn push.
    #[inline]
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// Check if the castle path is impeded for the current player. Does not assume
    /// the current player has the ability to castle, whether by having castling-rights
    /// or having the rook and king be on the correct squares. Also does not check legality
    /// (i.e. ensuring none of the king squares are in check).
    #[inline]
    pub fn castle_impeded(&self, castle_type: CastleType) -> bool {
        let path = Bitboard(CASTLING_PATH[self.turn as usize][castle_type as usize]);
        (path & self.occupied()).is_not_empty()
    }

    /// Check if the given player can castle to the given side.
    #[inline]
    pub fn can_castle(&self, player: Player, side: CastleType) -> bool {
        if player == Player::White {
            if side == CastleType::Kingside {
                self.castling_rights.white_kingside
            } else {
                self.castling_rights.white_queenside
            }
        } else {
            if side == CastleType::Kingside {
                self.castling_rights.black_kingside
            } else {
                self.castling_rights.black_queenside
            }
        }
    }

    #[inline]
    pub fn castling_rook_square(&self, side: CastleType) -> Square {
        Square(CASTLING_ROOK_START[self.turn() as usize][side as usize])
    }

    /// Returns the king square for the given player.
    #[inline]
    pub fn king_sq(&self, player: Player) -> Square {
        self.piece_bb(player, PieceType::King).to_square()
    }

    /// Returns the pinned pieces of the given player.
    ///
    /// Pinned is defined as pinned to the same players king
    #[inline(always)]
    pub fn pinned_pieces(&self, player: Player) -> Bitboard {
        self.state
            .as_ref()
            .expect("tried to check state when it was not set")
            .blockers[player as usize]
            & self.get_occupied_player(player)
    }

    // MOVE TESTING
    /// Tests if a given pseudo-legal move is legal. Used for checking the legality
    /// of moves that are generated as pseudo-legal in movegen. Pseudo-legal moves
    /// can create a discovered check, or the moving side can move into check. The case
    /// of castling through check is already dealt with in movegen.
    pub fn legal_move(&self, mov: Move) -> bool {
        if mov.is_none() || mov.is_null() {
            println!("here");
            return false;
        }

        let us = self.turn();
        let them = !us;
        let orig = mov.orig();
        let orig_bb = orig.to_bb();
        let dest = mov.dest();

        // En passant
        if mov.move_type() == SpecialMove::EnPassant {
            let ksq = self.king_sq(us);
            let dest_bb = dest.to_bb();
            let captured_sq = Square((dest.0 as i8).wrapping_sub(us.pawn_push()) as u8);
            // Work out the occupancy bb resulting from the en passant move
            let occupied = (self.occupied() ^ orig_bb ^ captured_sq.to_bb()) | dest_bb;

            return (rook_moves(occupied, ksq) & self.sliding_piece_bb(them)).is_empty()
                && (bishop_moves(occupied, ksq) & self.diagonal_piece_bb(them)).is_empty();
        }

        let piece = self.piece_at_sq(orig);
        if piece == Piece::None {
            return false;
        }

        // If moving the king, check if the destination square is not being attacked
        // Note: castling moves are already checked in movegen.
        if piece.type_of() == PieceType::King {
            return mov.move_type() == SpecialMove::Castling
                || (self.attackers_to(dest, self.occupied()) & self.get_occupied_player(them))
                    .is_empty();
        }
        // Ensure we are not moving a pinned piece
        (self.pinned_pieces(us) & orig_bb).is_empty() || aligned(orig, dest, self.king_sq(us))
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "")?;
        writeln!(f, "BITBOARDS\n=========\n")?;
        writeln!(f, "No Pieces:\n {}", self.no_piece)?;
        writeln!(f, "White Pawns:\n {}", self.white_pawns)?;
        writeln!(f, "White Knights:\n {}", self.white_knights)?;
        writeln!(f, "White Bishops:\n {}", self.white_bishops)?;
        writeln!(f, "White Rooks:\n {}", self.white_rooks)?;
        writeln!(f, "White Queens:\n {}", self.white_queens)?;
        writeln!(f, "White King:\n {}", self.white_king)?;
        writeln!(f, "Black Pawns:\n {}", self.black_pawns)?;
        writeln!(f, "Black Knights:\n {}", self.black_knights)?;
        writeln!(f, "Black Bishops:\n {}", self.black_bishops)?;
        writeln!(f, "Black Rooks:\n {}", self.black_rooks)?;
        writeln!(f, "Black Queens:\n {}", self.black_queens)?;
        writeln!(f, "Black King:\n {}", self.black_king)?;
        writeln!(f, "White Pieces:\n {}", self.white_pieces)?;
        writeln!(f, "Black Pieces:\n {}", self.black_pieces)?;

        writeln!(f, "BOARD ARRAY\n===========\n")?;
        writeln!(f, "{}", self.board)?;

        writeln!(f, "PIECE COUNTS\n============\n")?;
        writeln!(f, "White: {}", self.white_piece_count)?;
        writeln!(f, "Black: {}", self.black_piece_count)?;
        writeln!(f)?;

        writeln!(f, "INVISIBLE STATE\n===============\n")?;
        writeln!(f, "Turn: {}", self.turn())?;
        writeln!(f, "Castling Rights: {}", self.castling_rights)?;
        writeln!(
            f,
            "En Passant Square: {}",
            match self.ep_square {
                Some(sq) => sq.to_string(),
                None => "none".to_string(),
            }
        )?;
        writeln!(f, "Half move clock: {}", self.half_move_clock)?;
        writeln!(f, "Move number: {}", self.move_number)?;
        writeln!(f)?;
        writeln!(f, "STATE\n=====\n")?;

        if let Some(state) = &self.state {
            writeln!(f, "{}", state)
        } else {
            writeln!(f, "None")
        }
    }
}

/// For whatever rank the bit (inner value of a `Square`) is, returns the
/// corresponding rank as a u64.
#[inline(always)]
pub fn rank_bb(s: u8) -> u64 {
    RANK_BB[rank_idx_of_sq(s) as usize]
}

/// For whatever rank the bit (inner value of a `Square`) is, returns the
/// corresponding `Rank` index.
#[inline(always)]
pub fn rank_idx_of_sq(s: u8) -> u8 {
    (s >> 3) as u8
}

/// For whatever file the bit (inner value of a `Square`) is, returns the
/// corresponding file as a u64.
#[inline(always)]
pub fn file_bb(s: u8) -> u64 {
    FILE_BB[file_of_sq(s) as usize]
}

/// For whatever file the bit (inner value of a `Square`) is, returns the
/// corresponding file.
// TODO: make this return a dedicated `File` enum
#[inline(always)]
pub fn file_of_sq(s: u8) -> u8 {
    s & 0b0000_0111
}

/// Given a square (u8) that is valid, returns the bitboard representation
/// of that square.
///
/// # Safety
///
/// If the input is greater than 63, an empty u64 will be returned.
#[inline]
pub fn u8_to_u64(s: u8) -> u64 {
    debug_assert!(s < 64);
    (1 as u64).wrapping_shl(s as u32)
}
