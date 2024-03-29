mod board;
mod castling;
mod fen;
mod notation;
mod piece;
mod square;
mod state;
mod zobrist;

use crate::bb::Bitboard;
use crate::masks::{CASTLING_PATH, CASTLING_ROOK_START, FILE_BB, PLAYER_CNT, RANK_BB};
use crate::mono_traits::{All, Generate, Legal, Legality, Side};
use crate::mov::{Move, MoveType, UndoableMove};
use crate::movegen::{bishop_moves, queen_moves, rook_moves, MoveGen};
use crate::movelist::{BasicMoveList, Frame, MoveList, MoveStack};
use crate::precalc::boards::{aligned, between_bb, king_moves, knight_moves, pawn_attacks_from};

pub use board::Board;
pub use castling::{CastleType, CastlingRights};
pub use fen::{FenError, START_POSITION};
pub use piece::{Piece, PieceType, PIECE_TYPES, PROMO_PIECES};
pub use square::Square;
pub use state::State;
pub use zobrist::Zobrist;

use std::fmt;
use std::ops::Not;

/// The number of piece types including color on a chess board. Includes `Piece::None`.
pub const PIECE_TYPE_CNT: usize = 13;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Player(bool);

impl Player {
    pub const WHITE: Self = Self(false);
    pub const BLACK: Self = Self(true);

    /// Return the inner boolean.
    #[inline(always)]
    pub fn inner(&self) -> bool {
        self.0
    }

    /// Returns if the player is `Player::White`
    #[inline(always)]
    pub fn is_white(&self) -> bool {
        !self.0
    }

    /// Returns if the player is `Player::Black`
    #[inline(always)]
    pub fn is_black(&self) -> bool {
        !self.is_white()
    }

    /// Returns the other player.
    #[inline(always)]
    pub fn other_player(&self) -> Self {
        Self(!self.0)
    }

    /// Returns the relative square from a given square.
    #[inline(always)]
    pub fn relative_square(self, sq: Square) -> Square {
        assert!(sq.is_okay());
        sq ^ Square((self.0) as u8 * 56)
    }

    /// Returns the offset for a single move pawn push.
    #[inline(always)]
    pub fn pawn_push(self) -> i8 {
        match self {
            Player::WHITE => 8,
            Player::BLACK => -8,
        }
    }

    /// Returns the actual algebraic notation board rank for
    /// a given rank as seen from the `Player`s perspective.
    #[inline(always)]
    pub fn relative_rank(&self, rank: u8) -> u8 {
        debug_assert!(rank <= 7);
        match self.0 {
            false => rank,
            true => 7 - rank,
        }
    }
}

impl Not for Player {
    type Output = Self;
    #[inline(always)]
    fn not(self) -> Self::Output {
        self.other_player()
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            false => write!(f, "White"),
            true => write!(f, "Black"),
        }
    }
}

// TODO: turn off pub for all the `Position` fields and provide getters
#[derive(Clone, Eq, PartialEq)]
pub struct Position {
    /// Array of pieces on the board.
    pub(crate) board: Board,

    // Bitboards for each piece type
    /// Piece-type bitboards. One for each player/piece combination.
    pub(crate) bbs: [Bitboard; PIECE_TYPE_CNT],
    /// Occupancy bitboards for white and black.
    pub(crate) player_occ: [Bitboard; PLAYER_CNT],

    // "Invisible" state
    /// The side whose turn it is to move.
    turn: Player,
    /// The castling rights of both players.
    pub(crate) castling_rights: CastlingRights,
    /// A square on which an en passant capture is allowed, if any.
    pub(crate) ep_square: Option<Square>,
    /// The number of moves since the 50-move rule counter was last reset by a pawn move or
    /// capture.
    pub(crate) half_move_clock: u32,
    /// Full move number of the current position in the game being played.
    pub(crate) move_number: u32,

    /// `State` struct stores other useful information for fast access
    // TODO: Pleco wraps this in an Arc for quick copying of states without
    // copying memory. Do we need that?
    // TODO: This probably needs a better name since it really just has info
    // on pins, checks and blocks.
    pub(crate) state: State,

    /// History stores a `Vec` of `UndoableMove`s, allowing the `Position` to
    /// be rolled back with `unmake_move()`.
    pub(crate) history: Vec<UndoableMove>,

    /// The Zobrist key of the current position. Incrementally updated in `makemove()`
    /// and `unmakemove()`.
    pub(crate) zobrist: Zobrist,
}

impl Position {
    /// Creates a 'blank' `Position` struct. This method is safe to call even
    /// before `init_globals()`.
    pub fn blank() -> Self {
        Self {
            board: Board::new(),
            bbs: [Bitboard::new(0); PIECE_TYPE_CNT],
            player_occ: [Bitboard::new(0); PLAYER_CNT],
            turn: Player::WHITE,
            castling_rights: CastlingRights::none(),
            ep_square: None,
            half_move_clock: 0,
            move_number: 1,
            state: State::blank(),
            history: Vec::with_capacity(16),
            zobrist: Zobrist::empty(),
        }
    }

    /// Pretty print the board to stdout.
    pub fn pretty_print(&self) {
        println!("{}", self);
    }

    /// Sets the `State` struct for the current position. Should only be called
    /// when initialising a new `Position`.
    pub fn set_state(&mut self) {
        self.state = State::from_position(&self);
    }

    /// Set the `Zobrist` key for the current position based on the other data in
    /// the `Position` struct. Should only be called when initialising a new `Position`
    /// as the zobrist key is kept incrementally updated thereafter.
    pub fn set_zobrist(&mut self) {
        self.zobrist = Zobrist::from_position(&self);
    }

    pub fn history(&self) -> &Vec<UndoableMove> {
        &self.history
    }

    pub fn print_history(&self) -> String {
        let mut string = String::new();
        for mov in &self.history {
            let mov_str = format!("{} ", mov);
            string.push_str(&mov_str);
        }
        string
    }

    pub fn half_move_clock(&self) -> u32 {
        self.half_move_clock
    }

    pub fn zobrist(&self) -> Zobrist {
        self.zobrist
    }

    /// Make a move on the Board and update the `Position`.
    ///
    /// The supplied `Move` must be legal in the current position, otherwise undefined behaviour
    /// will occur.
    pub fn make_move(&mut self, mov: &Move) {
        // In debug mode, check the move isn't somehow null
        debug_assert_ne!(mov.orig(), mov.dest());

        // Add an undoable move to the position history
        let undoable_move = mov.to_undoable(&self);
        self.history.push(undoable_move);

        // Reset the en passant square
        self.zobrist.update_ep_square(self.ep_square, None);
        self.ep_square = None;

        let us = self.turn();
        let them = !us;
        let from = mov.orig();
        let to = mov.dest();
        let moving_piece = self.piece_at_sq(from);
        let captured_piece = if mov.is_en_passant() {
            Piece::make(them, PieceType::Pawn)
        } else {
            self.piece_at_sq(to)
        };

        // Sanity check
        debug_assert_eq!(moving_piece.player(), us);

        // Increment clocks
        self.half_move_clock += 1;
        if us == Player::BLACK {
            // Black is moving, so the full-move counter will increment
            self.move_number += 1;
        }

        // Toggle player to move in zobrist key
        self.zobrist.toggle_side_to_move();

        // Castling rights
        let new_castling_rights = self.castling_rights.update(from);
        self.zobrist
            .update_castling_rights(self.castling_rights, new_castling_rights);
        self.castling_rights = new_castling_rights;

        // Castling move
        if mov.is_castle() {
            // Sanity checks
            debug_assert_eq!(moving_piece.type_of(), PieceType::King);
            debug_assert_eq!(captured_piece.type_of(), PieceType::None);

            let mut r_orig = Square(0);
            let mut r_dest = Square(0);
            self.apply_castling(us, from, to, &mut r_orig, &mut r_dest);
        } else if captured_piece != Piece::None {
            let mut cap_sq = to;
            if captured_piece.type_of() == PieceType::Pawn {
                if mov.is_en_passant() {
                    match us {
                        Player::WHITE => cap_sq -= Square(8),
                        Player::BLACK => cap_sq += Square(8),
                    };

                    debug_assert_eq!(moving_piece.type_of(), PieceType::Pawn);
                    debug_assert_eq!(us.relative_rank(5), to.rank()); // `to` square is on "6th" rank from player's perspective
                    debug_assert_eq!(self.piece_at_sq(to), Piece::None);
                    debug_assert_eq!(
                        self.piece_at_sq(cap_sq).player_piece(),
                        (them, PieceType::Pawn)
                    );
                }
            }

            // Update the `Bitboard`s and `Piece` array
            self.remove_piece_c(captured_piece, cap_sq);

            // Reset the 50-move clock
            self.half_move_clock = 0;
        }

        if !mov.is_castle() {
            self.move_piece_c(moving_piece, from, to);
        }

        // Extra book-keeping for pawn moves
        if moving_piece.type_of() == PieceType::Pawn {
            if to.0 ^ from.0 == 16 {
                // Double push
                let poss_ep: u8 = (to.0 as i8 - us.pawn_push()) as u8;

                // Set en passant square if the moved pawn can be captured
                if (Bitboard(pawn_attacks_from(Square(poss_ep), us))
                    & self.piece_bb(them, PieceType::Pawn))
                .is_not_empty()
                {
                    self.zobrist
                        .update_ep_square(self.ep_square, Some(Square(poss_ep)));
                    self.ep_square = Some(Square(poss_ep));
                }
            } else if let Some(promo_piece_type) = mov.promo_piece_type() {
                let us_promo = Piece::make(us, promo_piece_type);
                self.remove_piece_c(moving_piece, to);
                self.put_piece_c(us_promo, to);
            }

            self.half_move_clock = 0;
        }

        // Update "invisible" state
        self.turn = them;
        self.state = State::from_position(&self);
    }

    /// Unmake the most recent move, returning the `Position` to the previous state.
    pub fn unmake_move(&mut self) -> Option<UndoableMove> {
        if let Some(undoable_move) = self.history.pop() {
            self.turn = !self.turn();
            let us = self.turn();
            let orig = undoable_move.orig;
            let dest = undoable_move.dest;
            let mut piece_on = self.piece_at_sq(dest);

            // Sanity check (only in debug mode) that the move makes sense.
            debug_assert!(self.piece_at_sq(orig) == Piece::None || undoable_move.is_castle());

            if undoable_move.is_promo() {
                debug_assert_eq!(piece_on.type_of(), undoable_move.promo_piece_type.unwrap());

                self.remove_piece_c(piece_on, dest);
                self.put_piece_c(Piece::make(us, PieceType::Pawn), dest);
                piece_on = Piece::make(us, PieceType::Pawn);
            }

            if undoable_move.is_castle() {
                self.undo_castling(us, orig, dest);
            } else {
                self.move_piece_c(piece_on, dest, orig);
                let captured_piece = undoable_move.captured;
                if !captured_piece.is_none() {
                    let mut cap_sq = dest;
                    if undoable_move.is_en_passant() {
                        match us {
                            Player::WHITE => cap_sq -= Square(8),
                            Player::BLACK => cap_sq += Square(8),
                        };
                    }
                    self.put_piece_c(Piece::make(!us, captured_piece), cap_sq);
                }
            }
            self.zobrist = undoable_move.zobrist;
            self.half_move_clock = undoable_move.prev_half_move_clock;
            self.ep_square = undoable_move.prev_ep_square;
            self.castling_rights = undoable_move.prev_castling_rights;
            self.state = undoable_move.state;

            if us == Player::BLACK {
                // unmaking a Black move, so decrement the whole move counter
                self.move_number -= 1;
            }

            Some(undoable_move)
        } else {
            None
        }
    }

    /// Helper function to apply a castling move for a given player.
    ///
    /// Takes in the player to castle, the original king square and the original rook square.
    /// The k_dst and r_dst squares are pointers to values, modifying them to have the correct king and
    /// rook destination squares.
    ///
    /// # Safety
    ///
    /// Assumes that k_orig and r_orig are legal squares, and the player can legally castle.
    fn apply_castling(
        &mut self,
        player: Player,
        k_orig: Square,      // Starting square of the King
        k_dest: Square,      // King destination square
        r_orig: &mut Square, // Origin square of the Rook. Passed in as `Square(0)` and modified by the function
        r_dest: &mut Square, // Destination square of Rook. Passed in as `Square(0)` and modified by the function
    ) {
        if k_orig < k_dest {
            // Kingside castling
            *r_orig = player.relative_square(Square::H1);
            *r_dest = player.relative_square(Square::F1);
        } else {
            // Queenside castling
            *r_orig = player.relative_square(Square::A1);
            *r_dest = player.relative_square(Square::D1);
        }
        self.move_piece_c(Piece::make(player, PieceType::King), k_orig, k_dest);
        self.move_piece_c(Piece::make(player, PieceType::Rook), *r_orig, *r_dest);
    }

    /// Helper function to undo a castling move for a given player.
    ///
    /// # Safety
    ///
    /// Undefined behaviour will result if calling this function when not unmaking an actual
    /// castling move.
    fn undo_castling(&mut self, player: Player, k_orig: Square, k_dest: Square) {
        let r_orig: Square;
        let r_dest: Square;
        if k_orig < k_dest {
            // Kingside castling
            r_orig = player.relative_square(Square::H1);
            r_dest = player.relative_square(Square::F1);
        } else {
            // Queenside castling
            r_orig = player.relative_square(Square::A1);
            r_dest = player.relative_square(Square::D1);
        }

        debug_assert_eq!(
            self.piece_at_sq(r_dest),
            Piece::make(player, PieceType::Rook)
        );
        debug_assert_eq!(
            self.piece_at_sq(k_dest),
            Piece::make(player, PieceType::King)
        );

        self.move_piece_c(Piece::make(player, PieceType::King), k_dest, k_orig);
        self.move_piece_c(Piece::make(player, PieceType::Rook), r_dest, r_orig);
    }

    /// Makes the given uci move on the board if it's legal.
    ///
    /// Returns `Option<Move>` with `Some(mov)` if the move was legal, and
    /// None if it wasn't.
    pub fn make_uci_move(&mut self, uci: &str) -> Option<Move> {
        let moves = self.generate::<BasicMoveList, All, Legal>();

        for mov in &moves {
            let uci_mov = mov.to_uci_string();
            if uci == uci_mov {
                self.make_move(mov);
                return Some(*mov);
            }
        }

        return None;
    }

    /// Moves a piece on the board for a given player from square `from`
    /// to square `to`. Updates all relevant `Bitboard` and the `Piece` array.
    ///
    /// # Panics
    ///
    /// Panics in debug mode if the two and from square are equal
    fn move_piece_c(&mut self, piece: Piece, from: Square, to: Square) {
        debug_assert_ne!(from, to);
        let comb_bb: Bitboard = from.to_bb() | to.to_bb();
        let (player, piece_ty) = piece.player_piece();
        self.bbs[Piece::None as usize] ^= comb_bb;
        self.bbs[piece as usize] ^= comb_bb;

        self.player_occ[player.inner() as usize] ^= comb_bb;

        self.board.remove(from);
        self.board.place(to, player, piece_ty);

        self.zobrist.toggle_piece_sq(piece, from);
        self.zobrist.toggle_piece_sq(piece, to);
    }

    /// Removes a `Piece` from the board for a given player.
    ///
    /// # Panics
    ///
    /// In debug mode, panics if there is not a `piece` at the given square.
    fn remove_piece_c(&mut self, piece: Piece, square: Square) {
        debug_assert_eq!(self.piece_at_sq(square), piece);
        let player = piece.player();
        let bb = square.to_bb();

        self.bbs[Piece::None as usize] ^= bb;
        self.bbs[piece as usize] ^= bb;

        self.player_occ[player.inner() as usize] ^= bb;

        self.board.remove(square);

        self.zobrist.toggle_piece_sq(piece, square);
    }

    /// Places a `Piece` on the board at a given `Square`.
    ///
    /// # Safety
    ///
    /// In debug mode, panics if there is already a piece at that `Square`.
    fn put_piece_c(&mut self, piece: Piece, square: Square) {
        debug_assert_eq!(self.piece_at_sq(square), Piece::None);

        let bb = square.to_bb();
        let (player, piece_ty) = piece.player_piece();
        self.bbs[Piece::None as usize] ^= bb;
        self.bbs[piece as usize] ^= bb;
        self.player_occ[player.inner() as usize] ^= bb;

        self.board.place(square, player, piece_ty);

        self.zobrist.toggle_piece_sq(piece, square);
    }

    // CHECKING
    /// Returns if current side to move is in check.
    #[inline(always)]
    pub fn in_check(&self) -> bool {
        self.state.checkers.is_not_empty()
    }

    /// Returns if opponent is in check. Such a state can never occur legally in chess. Sometimes
    /// we generate pseudolegal moves which, when made, cause this state to arise. Use this method
    /// to check if that has occurred and bail.
    #[inline(always)]
    pub fn enemy_in_check(&self) -> bool {
        (self.attack_defend(self.occupied(), self.king_sq(!self.turn()))
            & self.get_occupied_player_runtime(self.turn()))
        .is_not_empty()
    }

    /// Whether the player to move is in checkmate in this position.
    ///
    /// TODO: this should really switch `Generation` type to `Evasions`? Need to thoroughly check
    /// correctness when making that change.
    pub fn in_checkmate(&self) -> bool {
        self.in_check() && self.generate::<BasicMoveList, All, Legal>().is_empty()
    }

    pub fn in_double_check(&self) -> bool {
        self.state.checkers.popcnt() == 2
    }

    /// Determines whether the current position has occurred twice before in the history.
    ///
    /// Note: this does not exhaustively search for _any_ threefold repetition in the history of
    /// the position, only a threefold repetition where the final repetition is the _current_
    /// position.
    pub fn in_threefold(&self) -> bool {
        // TODO: here, we check that there are at least 4 moves in the history. However, if we are
        // conducting a search, we perhaps want to only go as far back as the root move, on the
        // basis that if we have been asked to search a position, we already know that the existing
        // threefold draw is not considered. This _could_ save time, because in practice, UCI means
        // that the entire game is in history, so we will usually have to go all the way to the
        // last pawn or capture move, which may be before the current search's root.

        if self.history().len() < 4 {
            return false;
        }

        let curr_zob = self.zobrist();
        let mut c = 0;

        for m in self.history().iter().rev().skip(1).step_by(2) {
            if m.ty.contains(MoveType::CAPTURE) || m.piece.type_of() == PieceType::Pawn {
                return false;
            }

            if m.zobrist == curr_zob {
                c += 1;

                if c == 2 {
                    return true;
                }
            }
        }

        false
    }

    /// Returns a `Bitboard` of possible attacks to a square with a given occupancy.
    /// Includes pieces from both players.
    // TODO: dedup this and `attack_defend`? which is faster?
    pub fn attackers_to(&self, sq: Square) -> Bitboard {
        (Bitboard(pawn_attacks_from(sq, Player::BLACK)) & self.bbs[1])
            | (Bitboard(pawn_attacks_from(sq, Player::WHITE)) & self.bbs[7])
            | (knight_moves(sq) & (self.bbs[2] | self.bbs[8]))
            | (rook_moves(self.occupied(), sq)
                & (self.bbs[4] | self.bbs[10] | self.bbs[5] | self.bbs[11]))
            | (bishop_moves(self.occupied(), sq)
                & (self.bbs[3] | self.bbs[9] | self.bbs[5] | self.bbs[11]))
            | (king_moves(sq) & (self.bbs[6] | self.bbs[12]))
    }

    #[inline(always)]
    pub fn turn(&self) -> Player {
        self.turn
    }

    #[inline(always)]
    pub fn move_number(&self) -> u32 {
        self.move_number
    }

    #[inline(always)]
    pub fn castling_rights(&self) -> CastlingRights {
        self.castling_rights
    }

    #[inline(always)]
    pub fn occupied(&self) -> Bitboard {
        !self.bbs[Piece::None as usize]
    }

    #[inline(always)]
    pub fn get_occupied<PL: Side>(&self) -> Bitboard {
        self.player_occ[PL::player().inner() as usize]
    }

    #[inline(always)]
    pub fn get_occupied_enemy<PL: Side>(&self) -> Bitboard {
        self.player_occ[PL::player().other_player().inner() as usize]
    }

    #[inline(always)]
    pub fn get_occupied_player_runtime(&self, player: Player) -> Bitboard {
        self.player_occ[player.inner() as usize]
    }

    #[inline(always)]
    pub fn occupied_white(&self) -> Bitboard {
        self.player_occ[Player::WHITE.inner() as usize]
    }

    #[inline(always)]
    pub fn occupied_black(&self) -> Bitboard {
        self.player_occ[Player::BLACK.inner() as usize]
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
        let other_occ = self.get_occupied_player_runtime(player_at);
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
        let idx = 6 * (player.inner() as usize) + (piece_type as usize);
        unsafe { *self.bbs.get_unchecked(idx) }
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
    #[inline(always)]
    pub fn piece_bb_both_players(&self, piece: PieceType) -> Bitboard {
        self.piece_bb(Player::WHITE, piece) | self.piece_bb(Player::BLACK, piece)
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

    /// Returns the checkers `Bitboard` for the current position.
    #[inline]
    pub fn checkers(&self) -> Bitboard {
        self.state.checkers
    }

    /// Check if the castle path is impeded for the current player. Does not assume
    /// the current player has the ability to castle, whether by having castling-rights
    /// or having the rook and king be on the correct squares. Also does not check legality
    /// (i.e. ensuring none of the king squares are in check).
    #[inline]
    pub fn castle_impeded(&self, castle_type: CastleType) -> bool {
        let path = Bitboard(CASTLING_PATH[self.turn.inner() as usize][castle_type as usize]);
        (path & self.occupied()).is_not_empty()
    }

    /// Check if the given player can castle to the given side.
    #[inline]
    pub fn can_castle(&self, player: Player, side: CastleType) -> bool {
        match player {
            Player::WHITE => match side {
                CastleType::Kingside => self.castling_rights.white_kingside(),
                CastleType::Queenside => self.castling_rights.white_queenside(),
            },
            Player::BLACK => match side {
                CastleType::Kingside => self.castling_rights.black_kingside(),
                CastleType::Queenside => self.castling_rights.black_queenside(),
            },
        }
    }

    #[inline]
    pub fn castling_rook_square(&self, side: CastleType) -> Square {
        Square(CASTLING_ROOK_START[self.turn().inner() as usize][side as usize])
    }

    /// Returns a bitboard of pieces attacking and defending a given square.
    ///
    /// This method is useful in places like Static Exchange Evaluation (SEE) and checking whether
    /// hash table moves or killer moves are (pseudo-)legal.
    ///
    /// Follows the implementation given [here](https://www.chessprogramming.org/Square_Attacked_By).
    pub fn attack_defend(&self, occ: Bitboard, sq: Square) -> Bitboard {
        let knights = self.piece_bb_both_players(PieceType::Knight);
        let kings = self.piece_bb_both_players(PieceType::King);
        let mut bishops_queens = self.piece_bb_both_players(PieceType::Queen);
        let mut rooks_queens = bishops_queens;
        bishops_queens |= self.piece_bb_both_players(PieceType::Bishop);
        rooks_queens |= self.piece_bb_both_players(PieceType::Rook);

        (Bitboard(pawn_attacks_from(sq, Player::WHITE))
            & self.piece_bb(Player::BLACK, PieceType::Pawn))
            | (Bitboard(pawn_attacks_from(sq, Player::BLACK))
                & self.piece_bb(Player::WHITE, PieceType::Pawn))
            | (knight_moves(sq) & knights)
            | (king_moves(sq) & kings)
            | (bishop_moves(occ, sq) & bishops_queens)
            | (rook_moves(occ, sq) & rooks_queens)
    }

    /// Returns a bitboard of sliding pieces attacking and defending a given square.
    pub fn attack_defend_sliding(&self, occ: Bitboard, sq: Square) -> Bitboard {
        let mut bishops_queens = self.piece_bb_both_players(PieceType::Queen);
        let mut rooks_queens = bishops_queens;
        bishops_queens |= self.piece_bb_both_players(PieceType::Bishop);
        rooks_queens |= self.piece_bb_both_players(PieceType::Rook);

        (bishop_moves(occ, sq) & bishops_queens) | (rook_moves(occ, sq) & rooks_queens)
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
        self.state.blockers[player.inner() as usize] & self.get_occupied_player_runtime(player)
    }

    // MOVE GENERATION
    /// Generate moves for the current position according to the generic parameters.
    #[inline]
    pub fn generate<ML: MoveList, G: Generate, L: Legality>(&self) -> ML {
        MoveGen::generate::<ML, G, L>(&self)
    }

    /// Generate moves for the current position according to the generic parameters. Moves are
    /// pushed into the passed `MoveList`.
    #[inline]
    pub fn generate_in<ML: MoveList, G: Generate, L: Legality>(&self, movelist: &mut ML) {
        MoveGen::generate_in::<ML, G, L>(&self, movelist);
    }

    #[inline]
    pub fn random_move(&self) -> Option<Move> {
        self.generate::<BasicMoveList, All, Legal>()
            .random()
            .copied()
    }

    // MOVE TESTING
    /// Tests if a given pseudo-legal move is legal. Used for checking the legality
    /// of moves that are generated as pseudo-legal in movegen. Pseudo-legal moves
    /// can create a discovered check, or the moving side can move into check. The case
    /// of castling through check is already dealt with in movegen.
    ///
    /// This method does not actually play the move on the board, but uses faster techniques
    /// to determine whether the move is legal.
    pub fn legal_move(&self, mov: &Move) -> bool {
        let them = !self.turn();
        let orig_bb = mov.orig().to_bb();
        let dest = mov.dest();

        // En passant
        if mov.move_type().contains(MoveType::EN_PASSANT) {
            let ksq = self.king_sq(self.turn());
            let dest_bb = dest.to_bb();
            let captured_sq = Square((dest.0 as i8).wrapping_sub(self.turn().pawn_push()) as u8);
            // Work out the occupancy bb resulting from the en passant move
            let occupied = (self.occupied() ^ orig_bb ^ captured_sq.to_bb()) | dest_bb;

            return (rook_moves(occupied, ksq) & self.sliding_piece_bb(them)).is_empty()
                && (bishop_moves(occupied, ksq) & self.diagonal_piece_bb(them)).is_empty();
        }

        let piece = self.piece_at_sq(mov.orig());

        // If moving the king, check if the destination square is not being attacked
        // Note: castling moves are already checked in movegen.
        if piece.type_of() == PieceType::King {
            return mov.move_type().contains(MoveType::CASTLE)
                || (self.attackers_to(dest) & self.get_occupied_player_runtime(them)).is_empty();
        }

        // Ensure we are not moving a pinned piece, or if we are, it is remaining staying
        // pinned but moving along the current rank, file, diagonal between the pinner and the king
        (self.pinned_pieces(self.turn()) & orig_bb).is_empty()
            || aligned(mov.orig(), dest, self.king_sq(self.turn()))
    }

    /// Tests if a move applies to the current position. This is useful when checking transposition
    /// table or killer table moves, when the stored result could possibly be completely wrong for
    /// the current position.
    ///
    /// This checks that the board has a piece of the correct colour at the origin square, and that
    /// that piece is able to move to the destination square. If so, it calls `self.legal_move` to
    /// further test for legality.
    pub fn valid_move(&self, mov: &Move) -> bool {
        // TODO: believe we can kill the second check
        MoveGen::valid_move(self, mov) && self.legal_move(mov)
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::start_pos()
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.board)?;
        writeln!(f, "fen: {}", self.to_fen())?;
        write!(f, "key: {:X}", self.zobrist())
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "")?;
        writeln!(f, "BITBOARDS\n=========\n")?;
        writeln!(f, "No Pieces:\n {}", self.bbs[Piece::None as usize])?;
        writeln!(
            f,
            "White Pawns:\n {}",
            self.piece_bb(Player::WHITE, PieceType::Pawn)
        )?;
        writeln!(
            f,
            "White Knights:\n {}",
            self.piece_bb(Player::WHITE, PieceType::Knight)
        )?;
        writeln!(
            f,
            "White Bishops:\n {}",
            self.piece_bb(Player::WHITE, PieceType::Bishop)
        )?;
        writeln!(
            f,
            "White Rooks:\n {}",
            self.piece_bb(Player::WHITE, PieceType::Rook)
        )?;
        writeln!(
            f,
            "White Queens:\n {}",
            self.piece_bb(Player::WHITE, PieceType::Queen)
        )?;
        writeln!(
            f,
            "White King:\n {}",
            self.piece_bb(Player::WHITE, PieceType::King)
        )?;
        writeln!(
            f,
            "Black Pawns:\n {}",
            self.piece_bb(Player::BLACK, PieceType::Pawn)
        )?;
        writeln!(
            f,
            "Black Knights:\n {}",
            self.piece_bb(Player::BLACK, PieceType::Knight)
        )?;
        writeln!(
            f,
            "Black Bishops:\n {}",
            self.piece_bb(Player::BLACK, PieceType::Bishop)
        )?;
        writeln!(
            f,
            "Black Rooks:\n {}",
            self.piece_bb(Player::BLACK, PieceType::Rook)
        )?;
        writeln!(
            f,
            "Black Queens:\n {}",
            self.piece_bb(Player::BLACK, PieceType::Queen)
        )?;
        writeln!(
            f,
            "Black King:\n {}",
            self.piece_bb(Player::BLACK, PieceType::King)
        )?;
        writeln!(f, "White Pieces:\n {}", self.occupied_white())?;
        writeln!(f, "Black Pieces:\n {}", self.occupied_black())?;

        writeln!(f, "BOARD ARRAY\n===========\n")?;
        writeln!(f, "{}", self.board)?;

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
        writeln!(f, "Zobrist key: {:b}", self.zobrist.0)?;
        writeln!(f)?;
        writeln!(f, "STATE\n=====\n")?;

        writeln!(f, "{}", self.state)?;
        writeln!(f)?;
        writeln!(f, "HISTORY\n=======")?;
        for mov in &self.history {
            write!(f, "{} ", mov)?;
        }
        writeln!(f)
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
