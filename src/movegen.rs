use crate::bb::Bitboard;
use crate::mono_traits::{
    BishopType, BlackType, KingType, KnightType, PieceTrait, PlayerTrait, QueenType, RookType,
    WhiteType,
};
use crate::mov::{Move, SpecialMove};
use crate::movelist::{MVPushable, MoveList};
use crate::position::{CastleType, PieceType, Player, Position, Square, PROMO_PIECES};
use crate::precalc::boards::{between_bb, king_moves, knight_moves, line_bb, pawn_attacks_from};
use crate::precalc::magic;

use std::ops::Index;

pub struct MoveGen {}

impl MoveGen {
    /// Generates pseudo-legal moves for the passed position.
    ///
    /// This function could return moves which are either:
    /// - Legal
    /// - Would cause a discovered check (i.e. the moving piece is pinned)
    /// - Would cause the moving king to land in check
    #[inline]
    pub fn generate(position: &Position) -> MoveList {
        let mut movelist = MoveList::default();
        InnerMoveGen::<MoveList>::generate(position, &mut movelist);
        movelist
    }

    /// Generates legal moves only, by first generating pseudo-legal
    /// moves with `generate()`, and then filtering through and eliminating
    /// those which do not pass a legality check.
    ///
    /// # Note
    ///
    /// This method is currently slow. We are using a `Vec` to collect
    /// the legal moves, which pushes things onto the heap. We should
    /// try to stick with the `MoveList` structure, which lives on the stack
    /// TODO: do that ^
    pub fn generate_legal(position: &Position) -> Vec<Move> {
        let pseudo_legal = Self::generate(position);
        let pseudo_legal_vec = pseudo_legal.vec();
        let mut legal_vec: Vec<Move> = Vec::new();
        for pseudo_mov in pseudo_legal_vec {
            if position.legal_move(pseudo_mov) {
                legal_vec.push(pseudo_mov);
            }
        }
        legal_vec
    }
}

pub struct InnerMoveGen<'a, MP: MVPushable + 'a> {
    movelist: &'a mut MP,
    position: &'a Position,
    /// All occupied squares on the board
    occ: Bitboard,
    /// Squares occupied by the player to move
    us_occ: Bitboard,
    /// Square occupied by the opponent
    them_occ: Bitboard,
}

impl<'a, MP: MVPushable> InnerMoveGen<'a, MP>
where
    <MP as Index<usize>>::Output: Sized,
{
    /// Generate all pseudo-legal moves in the given position
    // TODO: use the monorphization technique to generalise this over desired legality status
    // of the moves. So you can ask for only totally legal, or pseudo-legal.
    fn generate(position: &'a Position, movelist: &'a mut MP) -> &'a mut MP {
        match position.turn() {
            Player::White => InnerMoveGen::<MP>::generate_helper::<WhiteType>(position, movelist),
            Player::Black => InnerMoveGen::<MP>::generate_helper::<BlackType>(position, movelist),
        }
    }

    #[inline(always)]
    fn get_self(position: &'a Position, movelist: &'a mut MP) -> Self {
        InnerMoveGen {
            movelist,
            position,
            occ: position.occupied(),
            us_occ: position.get_occupied_player(position.turn()),
            them_occ: position.get_occupied_player(position.turn().other_player()),
        }
    }

    fn generate_helper<P: PlayerTrait>(position: &'a Position, movelist: &'a mut MP) -> &'a mut MP {
        let mut movegen = InnerMoveGen::<MP>::get_self(position, movelist);
        if movegen.position.in_check() {
            movegen.generate_evasions::<P>();
        } else {
            movegen.generate_all::<P>();
        }
        movegen.movelist
    }

    fn generate_all<P: PlayerTrait>(&mut self) {
        self.generate_pawn_moves::<P>(Bitboard::ALL);
        self.generate_castling::<P>();
        self.moves_per_piece::<P, KnightType>(Bitboard::ALL);
        self.moves_per_piece::<P, KingType>(Bitboard::ALL);
        self.moves_per_piece::<P, RookType>(Bitboard::ALL);
        self.moves_per_piece::<P, BishopType>(Bitboard::ALL);
        self.moves_per_piece::<P, QueenType>(Bitboard::ALL);
    }

    fn generate_evasions<P: PlayerTrait>(&mut self) {
        debug_assert!(self.position.in_check());

        let ksq = self.position.king_sq(P::player());
        let mut slider_attacks = Bitboard(0);

        // Pieces that could possibly attack the king with sliding attacks
        let mut sliders = self.position.checkers()
            & !self
                .position
                .piece_two_bb_both_players(PieceType::Pawn, PieceType::Knight);

        // All the squares that are attacked by sliders
        while let Some((check_sq, check_sq_bb)) = sliders.pop_some_lsb_and_bit() {
            slider_attacks |= Bitboard(line_bb(check_sq, ksq)) ^ check_sq_bb;
        }

        // Possible king moves, where the king cannot move into a slider / own pieces
        let k_moves = king_moves(ksq) & !slider_attacks & !self.us_occ;

        // Separate captures and non-captures
        let mut captures_bb = k_moves & self.them_occ;
        let mut non_captures_bb = k_moves & !self.them_occ;
        self.move_append_from_bb_flag(&mut captures_bb, ksq, SpecialMove::Capture);
        self.move_append_from_bb_flag(&mut non_captures_bb, ksq, SpecialMove::Quiet);

        // If there is only one checking square, we can block or capture the piece
        if !(self.position.checkers().more_than_one()) {
            let checking_sq = Square(self.position.checkers().bsf() as u8);

            // Squares that allow a block or captures of the sliding piece
            let target = Bitboard(between_bb(checking_sq, ksq)) | checking_sq.to_bb();
            self.generate_pawn_moves::<P>(target);
            self.moves_per_piece::<P, KnightType>(target);
            self.moves_per_piece::<P, BishopType>(target);
            self.moves_per_piece::<P, RookType>(target);
            self.moves_per_piece::<P, QueenType>(target);
        }
    }

    fn moves_per_piece<PL: PlayerTrait, P: PieceTrait>(&mut self, target: Bitboard) {
        let piece_bb: Bitboard = self.position.piece_bb(PL::player(), P::piece_type());
        for orig in piece_bb {
            let moves_bb: Bitboard = self.moves_bb::<P>(orig) & !self.us_occ & target;
            let mut captures_bb: Bitboard = moves_bb & self.them_occ;
            let mut non_captures_bb: Bitboard = moves_bb & !self.them_occ;
            self.move_append_from_bb_flag(&mut captures_bb, orig, SpecialMove::Capture);
            self.move_append_from_bb_flag(&mut non_captures_bb, orig, SpecialMove::Quiet);
        }
    }

    fn generate_pawn_moves<PL: PlayerTrait>(&mut self, target: Bitboard) {
        let (rank_8, rank_7, rank_3): (Bitboard, Bitboard, Bitboard) =
            if PL::player() == Player::White {
                (Bitboard::RANK_8, Bitboard::RANK_7, Bitboard::RANK_3)
            } else {
                (Bitboard::RANK_1, Bitboard::RANK_2, Bitboard::RANK_6)
            };

        let all_pawns = self.position.piece_bb(PL::player(), PieceType::Pawn);

        // Separated out for promotion moves
        let pawns_rank_7: Bitboard = all_pawns & rank_7;

        // Separated out for non promotion moves
        let pawns_not_rank_7: Bitboard = all_pawns & !rank_7;

        let enemies = self.them_occ;

        // Single and double pawn moves
        let empty_squares = !self.position.occupied();

        let mut push_one = empty_squares & PL::shift_up(pawns_not_rank_7);
        let mut push_two = PL::shift_up(push_one & rank_3) & empty_squares;

        push_one &= target;
        push_two &= target;

        for dest in push_one {
            let orig = PL::down(dest);
            self.add_move(Move::build(orig, dest, None, SpecialMove::Quiet));
        }

        for dest in push_two {
            let orig = PL::down(PL::down(dest));
            self.add_move(Move::build(orig, dest, None, SpecialMove::Quiet));
        }

        // Promotions
        if pawns_rank_7.is_not_empty() {
            let no_cap_promo = target & PL::shift_up(pawns_rank_7) & empty_squares;
            let left_cap_promo = target & PL::shift_up_left(pawns_rank_7) & enemies;
            let right_cap_promo = target & PL::shift_up_right(pawns_rank_7) & enemies;

            for dest in no_cap_promo {
                let orig = PL::down(dest);
                self.add_all_promo_moves(orig, dest);
            }

            for dest in left_cap_promo {
                let orig = PL::down_right(dest);
                self.add_all_promo_moves(orig, dest);
            }

            for dest in right_cap_promo {
                let orig = PL::down_left(dest);
                self.add_all_promo_moves(orig, dest);
            }
        }

        // Captures
        let left_cap = target & PL::shift_up_left(pawns_not_rank_7) & enemies;
        let right_cap = target & PL::shift_up_right(pawns_not_rank_7) & enemies;

        for dest in left_cap {
            let orig = PL::down_right(dest);
            self.add_move(Move::build(orig, dest, None, SpecialMove::Capture));
        }

        for dest in right_cap {
            let orig = PL::down_left(dest);
            self.add_move(Move::build(orig, dest, None, SpecialMove::Capture));
        }

        if let Some(ep_square) = self.position.ep_square() {
            // TODO: add an `assert_eq` to check that the rank of ep_square is 6th
            // rank from the moving player's perspective

            let ep_cap =
                pawns_not_rank_7 & Bitboard(pawn_attacks_from(ep_square, PL::opp_player()));

            for orig in ep_cap {
                self.add_move(Move::build(orig, ep_square, None, SpecialMove::EnPassant));
            }
        }
    }

    // Generates castling for both sides
    fn generate_castling<PL: PlayerTrait>(&mut self) {
        self.castling_side::<PL>(CastleType::Queenside);
        self.castling_side::<PL>(CastleType::Kingside);
    }

    // Generates castling for a single side
    fn castling_side<PL: PlayerTrait>(&mut self, side: CastleType) {
        if self.position.can_castle(PL::player(), side)
            && !self.position.castle_impeded(side)
            && self
                .position
                .piece_at_sq(self.position.castling_rook_square(side))
                .type_of()
                == PieceType::Rook
        {
            let king_side = side == CastleType::Kingside;
            let ksq = self.position.king_sq(PL::player());
            // let rook_from = self.position.castling_rook_square(side);
            let k_to =
                PL::player().relative_square(if king_side { Square::G1 } else { Square::C1 });
            let enemies = self.them_occ;
            let direction: fn(Square) -> Square = if king_side {
                |x: Square| x - Square(1)
            } else {
                |x: Square| x + Square(1)
            };

            let mut s: Square = k_to;
            let mut can_castle = true;
            // Loop through all the squares the king goes through
            // If any enemies attack that square, cannot castle
            'outer: while s != ksq {
                let attackers = self.position.attackers_to(s, self.occ) & enemies;
                if attackers.is_not_empty() {
                    can_castle = false;
                    break 'outer;
                }
                s = direction(s);
            }
            if can_castle {
                self.add_move(Move::build(ksq, k_to, None, SpecialMove::Castling));
            }
        }
    }

    fn moves_bb<P: PieceTrait>(&mut self, sq: Square) -> Bitboard {
        debug_assert!(sq.is_okay());
        debug_assert_ne!(P::piece_type(), PieceType::Pawn);
        match P::piece_type() {
            PieceType::None => panic!(), // TODO
            PieceType::Pawn => panic!(),
            PieceType::Knight => knight_moves(sq),
            PieceType::Bishop => bishop_moves(self.occ, sq),
            PieceType::Rook => rook_moves(self.occ, sq),
            PieceType::Queen => queen_moves(self.occ, sq),
            PieceType::King => king_moves(sq),
        }
    }

    // TODO: make `Move` struct have a dedicated `SpecialMove` field, and introduce new enum
    // variants for 'capturing', 'quiet' etc.
    #[inline]
    fn move_append_from_bb_flag(&mut self, bb: &mut Bitboard, orig: Square, flag: SpecialMove) {
        for dest in bb {
            let mov = Move::build(orig, dest, None, flag);
            self.add_move(mov);
        }
    }

    #[inline]
    fn add_all_promo_moves(&mut self, orig: Square, dest: Square) {
        for piece in PROMO_PIECES {
            self.add_move(Move::build(orig, dest, Some(piece), SpecialMove::Promotion));
        }
    }

    #[inline(always)]
    fn add_move(&mut self, mv: Move) {
        self.movelist.push(mv);
    }
}

// MAGIC FUNCTIONS

/// Generate bishop moves `Bitboard` from a square and an occupancy bitboard.
/// This function will return captures to pieces on both sides. The resulting `Bitboard` must be
/// AND'd with the inverse of the moving player's pieces.
#[inline(always)]
pub fn bishop_moves(occupied: Bitboard, sq: Square) -> Bitboard {
    debug_assert!(sq.is_okay());
    Bitboard(magic::bishop_attacks(occupied.0, sq.0))
}

/// Generate rook moves `Bitboard` from a square and an occupancy bitboard.
/// This function will return captures to pieces on both sides. The resulting `Bitboard` must be
/// AND'd with the inverse of the moving player's pieces.#[inline(always)]
pub fn rook_moves(occupied: Bitboard, sq: Square) -> Bitboard {
    debug_assert!(sq.is_okay());
    Bitboard(magic::rook_attacks(occupied.0, sq.0))
}

/// Generate queen moves `Bitboard` from a square and an occupancy bitboard.
/// This function will return captures to pieces on both sides. The resulting `Bitboard` must be
/// AND'd with the inverse of the moving player's pieces.
#[inline(always)]
pub fn queen_moves(occupied: Bitboard, sq: Square) -> Bitboard {
    debug_assert!(sq.is_okay());
    Bitboard(magic::rook_attacks(occupied.0, sq.0) | magic::bishop_attacks(occupied.0, sq.0))
}
