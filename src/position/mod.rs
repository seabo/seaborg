mod board;
mod castling;
mod fen;
mod piece;
mod square;

use crate::bb::Bitboard;
use crate::masks::{CASTLING_PATH, CASTLING_ROOK_START, FILE_BB, RANK_BB};
use crate::movegen::{bishop_moves, rook_moves};
use crate::precalc::boards::{king_moves, knight_moves, pawn_attacks_from};

pub use board::Board;
pub use castling::{CastleType, CastlingRights};
pub use piece::{Piece, PieceType, PROMO_PIECES};
pub use square::Square;

use std::fmt;

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
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Player::White => write!(f, "White"),
            Player::Black => write!(f, "Black"),
        }
    }
}

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

    // Other state
    turn: Player,
    pub(crate) castling_rights: CastlingRights,
    pub(crate) ep_square: Option<Square>,
    pub(crate) half_move_clock: u32,
    pub(crate) move_number: u32,
}

impl Position {
    // CHECKING

    /// Returns a `Bitboard` of possible attacks to a square with a given occupancy.
    /// Includes pieces from both players.
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

    /// Returns the combined Bitboard of both players for a given piece.
    #[inline(always)]
    pub fn piece_bb_both_players(&self, piece: PieceType) -> Bitboard {
        match piece {
            PieceType::None => Bitboard(0),
            PieceType::Pawn => self.white_pawns & self.black_pawns,
            PieceType::Knight => self.white_knights & self.black_knights,
            PieceType::Bishop => self.white_bishops & self.black_bishops,
            PieceType::Rook => self.white_rooks & self.black_rooks,
            PieceType::Queen => self.white_queens & self.black_queens,
            PieceType::King => self.white_king & self.black_king,
        }
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
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Piece::None => write!(f, "."),
            Piece::WhitePawn => write!(f, "P"),
            Piece::WhiteKnight => write!(f, "N"),
            Piece::WhiteBishop => write!(f, "B"),
            Piece::WhiteRook => write!(f, "R"),
            Piece::WhiteQueen => write!(f, "Q"),
            Piece::WhiteKing => write!(f, "K"),
            Piece::BlackPawn => write!(f, "p"),
            Piece::BlackKnight => write!(f, "n"),
            Piece::BlackBishop => write!(f, "b"),
            Piece::BlackRook => write!(f, "r"),
            Piece::BlackQueen => write!(f, "q"),
            Piece::BlackKing => write!(f, "k"),
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
