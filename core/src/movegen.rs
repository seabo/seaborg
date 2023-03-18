use crate::bb::Bitboard;
use crate::mono_traits::{
    All, Bishop, Black, Captures, Generate, King, Knight, Legal, Legality, Pawn, PieceTrait,
    Promotions, PseudoLegal, Queen, QueenPromotions, Quiets, Rook, Side, White,
};
use crate::mov::{Move, MoveType};
use crate::movelist::{BasicMoveList, Frame, MoveList, MoveStack};
use crate::position::{CastleType, PieceType, Player, Position, Square, PROMO_PIECES};
use crate::precalc::boards::{between_bb, king_moves, knight_moves, line_bb, pawn_attacks_from};
use crate::precalc::magic;

/// Types of move generating options.
///
/// `Generation::All` -> All available moves.
///
/// `Generation::Captures` -> All captures which are not also promotions.
///
/// `Generation::Promotions` -> All promotions.
///
/// `Generation::QueenPromotions` -> All promotions to a queen only (i.e. no underpromotions).
///
/// `Generation::Quiets` -> All moves which are not promotions or captures.
///
/// # Safety
///
/// `Generation::QuietChecks` and `Generation::NonEvasions` can only be used if the board
/// if not in check, while `Generation::Evasions` can only be used if the the board is
/// in check. The remaining `Generation` can be used legally whenever.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Generation {
    All,
    Captures,
    Promotions,
    QueenPromotions,
    Quiets,
}

/// Legality of moves to be generated.
///
/// `LegalityKind::Legal` -> Generate only legal moves.
///
/// `LegalityKind::Pseudolegal` -> Generate both legal and pseudolegal moves. Pseudolegal moves
/// include those which cause a discovered check or cause the moving king to land in check.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LegalityKind {
    Legal,
    Pseudolegal,
}

pub struct MoveGen {}

impl MoveGen {
    /// Generate moves for the passed position according to the parameters specified by the dummy
    /// passed as generic types.
    #[inline]
    pub fn generate<ML: MoveList, G: Generate, L: Legality>(position: &Position) -> ML {
        let mut movelist = ML::empty();
        InnerMoveGen::<ML>::generate::<G, L>(position, &mut movelist);
        movelist
    }

    /// Generate moves for the passed position according to the parameters specified by the dummy
    /// passed as generic types.
    #[inline]
    pub fn generate_in<ML: MoveList, G: Generate, L: Legality>(
        position: &Position,
        movelist: &mut ML,
    ) {
        InnerMoveGen::<ML>::generate::<G, L>(position, movelist);
    }

    /// Determine whether the passed move is a valid pseudolegal move in the given position. This
    /// means that the move may leave the king in check. Use this to determine if a move retrieved
    /// from transposition table or killer tables etc. are actually valid for the position.
    pub fn valid_move(position: &Position, mov: &Move) -> bool {
        InnerMoveGen::<DummyMoveList>::valid_move::<All, Legal>(position, mov, &mut DummyMoveList)
    }
}

pub struct InnerMoveGen<'a, MP: MoveList + 'a> {
    movelist: &'a mut MP,
    position: &'a Position,
    /// All occupied squares on the board
    occ: Bitboard,
    /// Squares occupied by the player to move
    us_occ: Bitboard,
    /// Squares occupied by the opponent
    them_occ: Bitboard,
}

#[derive(Debug)]
struct DummyMoveList;
impl MoveList for DummyMoveList {
    fn len(&self) -> usize {
        0
    }

    fn push(&mut self, _mv: Move) {}
    fn empty() -> Self {
        Self
    }
    fn clear(&mut self) {}
}

impl<'a, MP: MoveList> InnerMoveGen<'a, MP> {
    /// Determine whether the passed move is a valid pseudolegal move in the given position.
    #[inline]
    fn valid_move<G: Generate, L: Legality>(
        position: &'a Position,
        mov: &'a Move,
        movelist: &mut MP,
    ) -> bool {
        match position.turn() {
            Player::WHITE => {
                InnerMoveGen::<MP>::valid_move_helper::<G, L, White>(position, mov, movelist)
            }
            Player::BLACK => {
                InnerMoveGen::<MP>::valid_move_helper::<G, L, Black>(position, mov, movelist)
            }
        }
    }

    #[inline]
    fn valid_move_helper<G: Generate, L: Legality, PL: Side>(
        position: &'a Position,
        mov: &'a Move,
        movelist: &'a mut MP,
    ) -> bool {
        let piece = position.piece_at_sq(mov.orig());
        let orig = mov.orig();
        let dest_bb = mov.dest().to_bb();
        let mut movegen = Self::get_self::<PL>(position, movelist);

        if (mov.move_type().contains(MoveType::CAPTURE))
            && (position.get_occupied_enemy::<PL>() & dest_bb).is_empty()
            || ((position.get_occupied_enemy::<PL>() & dest_bb).is_not_empty()
                && !mov.move_type().contains(MoveType::CAPTURE))
        {
            return false;
        }

        if (mov.move_type().contains(MoveType::CASTLE)) && piece.type_of() != PieceType::King {
            return false;
        }

        if movegen.position.in_check() {
            if piece.is_none() || piece.player() != movegen.position.turn() {
                return false;
            }

            return match piece.type_of() {
                PieceType::None => false,
                PieceType::Pawn => movegen.valid_evasion::<G, PL, L, Pawn>(dest_bb, orig),
                PieceType::Rook => movegen.valid_evasion::<G, PL, L, Rook>(dest_bb, orig),
                PieceType::Knight => movegen.valid_evasion::<G, PL, L, Knight>(dest_bb, orig),
                PieceType::Bishop => movegen.valid_evasion::<G, PL, L, Bishop>(dest_bb, orig),
                PieceType::Queen => movegen.valid_evasion::<G, PL, L, Queen>(dest_bb, orig),
                PieceType::King => movegen.valid_evasion::<G, PL, L, King>(dest_bb, orig),
            };
        }

        match piece.type_of() {
            PieceType::None => false,
            PieceType::Pawn => movegen.valid_pawn_move::<G, PL, L>(dest_bb, orig.to_bb()),
            PieceType::Rook => movegen.valid_move_per_piece::<G, PL, Rook, L>(dest_bb, orig),
            PieceType::Knight => movegen.valid_move_per_piece::<G, PL, Knight, L>(dest_bb, orig),
            PieceType::Bishop => movegen.valid_move_per_piece::<G, PL, Bishop, L>(dest_bb, orig),
            PieceType::Queen => movegen.valid_move_per_piece::<G, PL, Queen, L>(dest_bb, orig),
            PieceType::King => movegen.valid_move_per_piece::<G, PL, King, L>(dest_bb, orig),
        }
    }

    /// Generate all pseudo-legal moves in the given position
    #[inline(always)]
    fn generate<G: Generate, L: Legality>(
        position: &'a Position,
        movelist: &'a mut MP,
    ) -> &'a mut MP {
        match position.turn() {
            Player::WHITE => InnerMoveGen::<MP>::generate_helper::<G, L, White>(position, movelist),
            Player::BLACK => InnerMoveGen::<MP>::generate_helper::<G, L, Black>(position, movelist),
        }
    }

    #[inline(always)]
    fn get_self<PL: Side>(position: &'a Position, movelist: &'a mut MP) -> Self {
        InnerMoveGen {
            movelist,
            position,
            occ: position.occupied(),
            us_occ: position.get_occupied::<PL>(),
            them_occ: position.get_occupied_enemy::<PL>(),
        }
    }

    #[inline(always)]
    fn generate_helper<G: Generate, L: Legality, PL: Side>(
        position: &'a Position,
        movelist: &'a mut MP,
    ) -> &'a mut MP {
        let mut movegen = InnerMoveGen::<MP>::get_self::<PL>(position, movelist);
        let gen_type = G::kind();

        if movegen.position.in_check() {
            movegen.generate_evasions::<G, PL, L>();
            return movegen.movelist;
        }

        use Generation::*;
        match gen_type {
            All => {
                movegen.generate_all::<PL, L>();
            }
            Captures => {
                movegen.generate_captures::<PL, L>();
            }
            Promotions => {
                movegen.generate_promotions::<PL, L>();
            }
            QueenPromotions => {
                movegen.generate_queen_promotions::<PL, L>();
            }
            Quiets => {
                movegen.generate_quiets::<PL, L>();
            }
        }

        movegen.movelist
    }

    #[inline(always)]
    fn generate_all<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<All, P, L>(Bitboard::ALL);
        self.generate_castling::<P, L>();
        self.moves_per_piece::<All, P, Knight, L>(Bitboard::ALL);
        self.moves_per_piece::<All, P, King, L>(Bitboard::ALL);
        self.moves_per_piece::<All, P, Rook, L>(Bitboard::ALL);
        self.moves_per_piece::<All, P, Bishop, L>(Bitboard::ALL);
        self.moves_per_piece::<All, P, Queen, L>(Bitboard::ALL);
    }

    #[inline(always)]
    fn generate_promotions<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<Promotions, P, L>(Bitboard::ALL);
    }

    #[inline(always)]
    fn generate_queen_promotions<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<QueenPromotions, P, L>(Bitboard::ALL);
    }

    #[inline(always)]
    fn generate_captures<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<Captures, P, L>(self.them_occ);
        self.moves_per_piece::<Captures, P, Knight, L>(self.them_occ);
        self.moves_per_piece::<Captures, P, King, L>(self.them_occ);
        self.moves_per_piece::<Captures, P, Rook, L>(self.them_occ);
        self.moves_per_piece::<Captures, P, Bishop, L>(self.them_occ);
        self.moves_per_piece::<Captures, P, Queen, L>(self.them_occ);
    }

    #[inline(always)]
    fn generate_quiets<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<Quiets, P, L>(Bitboard::ALL);
        self.generate_castling::<P, L>();
        self.moves_per_piece::<Quiets, P, Knight, L>(Bitboard::ALL);
        self.moves_per_piece::<Quiets, P, King, L>(Bitboard::ALL);
        self.moves_per_piece::<Quiets, P, Rook, L>(Bitboard::ALL);
        self.moves_per_piece::<Quiets, P, Bishop, L>(Bitboard::ALL);
        self.moves_per_piece::<Quiets, P, Queen, L>(Bitboard::ALL);
    }

    #[inline(always)]
    fn generate_evasions<G: Generate, P: Side, L: Legality>(&mut self) {
        debug_assert!(self.position.in_check());

        let target_sqs = if G::kind() == Generation::Captures {
            self.them_occ
        } else if G::kind() == Generation::Quiets {
            !self.them_occ
        } else {
            Bitboard::ALL
        };

        let ksq = self.position.king_sq(P::player());

        // Only generate the king escapes if we are _not_ doing promotion moves.
        if G::kind() != Generation::Promotions && G::kind() != Generation::QueenPromotions {
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
            let k_moves = king_moves(ksq) & !slider_attacks & !self.us_occ & target_sqs;

            // Separate captures and non-captures
            if G::kind() == Generation::All || G::kind() == Generation::Captures {
                let mut captures_bb = k_moves & self.them_occ;
                self.move_append_from_bb_flag::<L>(&mut captures_bb, ksq, MoveType::CAPTURE);
            }

            if G::kind() != Generation::Captures {
                let mut non_captures_bb = k_moves & !self.them_occ;
                self.move_append_from_bb_flag::<L>(&mut non_captures_bb, ksq, MoveType::QUIET);
            }
        }

        // If there is only one checking square, we can block or capture the piece
        if !(self.position.checkers().more_than_one()) {
            let checking_sq = Square(self.position.checkers().bsf() as u8);

            // Squares that allow a block or captures of the sliding piece
            let target =
                target_sqs & (Bitboard(between_bb(checking_sq, ksq)) | checking_sq.to_bb());
            self.generate_pawn_moves::<G, P, L>(target);

            if G::kind() != Generation::Promotions && G::kind() != Generation::QueenPromotions {
                self.moves_per_piece::<G, P, Knight, L>(target);
                self.moves_per_piece::<G, P, Bishop, L>(target);
                self.moves_per_piece::<G, P, Rook, L>(target);
                self.moves_per_piece::<G, P, Queen, L>(target);
            }
        }
    }

    #[inline(always)]
    fn valid_evasion<G: Generate, PL: Side, L: Legality, P: PieceTrait>(
        &mut self,
        target: Bitboard,
        orig: Square,
    ) -> bool {
        debug_assert!(self.position.in_check());

        let ksq = self.position.king_sq(PL::player());

        if P::kind() == PieceType::King {
            if ksq != orig {
                return false;
            }

            // Only generate the king escapes if we are _not_ doing promotion moves.
            if G::kind() != Generation::Promotions && G::kind() != Generation::QueenPromotions {
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
                let k_moves = king_moves(ksq) & !slider_attacks & !self.us_occ & target;

                // Separate captures and non-captures
                if k_moves.is_not_empty() {
                    return true;
                }
            }
        } else {
            // If there is only one checking square, we can block or capture the piece
            if !(self.position.checkers().more_than_one()) {
                let checking_sq = Square(self.position.checkers().bsf() as u8);

                // Squares that allow a block or captures of the sliding piece
                let tgt = target & (Bitboard(between_bb(checking_sq, ksq)) | checking_sq.to_bb());

                return match P::kind() {
                    PieceType::None => unreachable!(), // checked for this earlier
                    PieceType::Pawn => self.valid_pawn_move::<G, PL, L>(tgt, orig.to_bb()),
                    PieceType::Rook => self.valid_move_per_piece::<G, PL, Rook, L>(tgt, orig),
                    PieceType::Knight => self.valid_move_per_piece::<G, PL, Knight, L>(tgt, orig),
                    PieceType::Bishop => self.valid_move_per_piece::<G, PL, Bishop, L>(tgt, orig),
                    PieceType::Queen => self.valid_move_per_piece::<G, PL, Queen, L>(tgt, orig),
                    PieceType::King => self.valid_move_per_piece::<G, PL, King, L>(tgt, orig),
                };
            }
        }

        false
    }

    /// Generate the moves for a `Knight`, `King`, `Rook`, `Bishop` or `Queen`. Generates either
    /// captures or non-captures, according to the generic parameter `G: Generate`.
    ///
    /// * `All` -> both captures and non-captures are generated.
    /// * `Captures` -> only captures are generated.
    /// * `Quiets` -> only non-captures are generated.
    /// * `Promotions` | `QueenPromotions` -> nothing is generated.
    #[inline(always)]
    fn moves_per_piece<G: Generate, PL: Side, P: PieceTrait, L: Legality>(
        &mut self,
        target: Bitboard,
    ) {
        let piece_bb: Bitboard = self.position.piece_bb(PL::player(), P::kind());
        for orig in piece_bb {
            let moves_bb: Bitboard = self.moves_bb::<P>(orig) & !self.us_occ & target;

            if G::kind() == Generation::All || G::kind() == Generation::Captures {
                let mut captures_bb: Bitboard = moves_bb & self.them_occ;
                self.move_append_from_bb_flag::<L>(&mut captures_bb, orig, MoveType::CAPTURE);
            }

            if G::kind() == Generation::All || G::kind() == Generation::Quiets {
                let mut non_captures_bb: Bitboard = moves_bb & !self.them_occ;
                self.move_append_from_bb_flag::<L>(&mut non_captures_bb, orig, MoveType::QUIET);
            }
        }
    }

    #[inline(always)]
    fn valid_move_per_piece<G: Generate, PL: Side, P: PieceTrait, L: Legality>(
        &mut self,
        target: Bitboard,
        orig: Square,
    ) -> bool {
        let piece_bb = self.position.piece_bb(PL::player(), P::kind()) & orig.to_bb();
        if piece_bb.is_empty() {
            return false;
        }

        let moves_bb = self.moves_bb::<P>(orig) & !self.us_occ & target;

        if G::kind() == Generation::Captures {
            (moves_bb & self.them_occ).is_not_empty()
        } else if G::kind() == Generation::All || G::kind() == Generation::Quiets {
            moves_bb.is_not_empty()
        } else {
            // We don't call this function for other generation types.
            unreachable!()
        }
    }

    #[inline(always)]
    fn generate_pawn_moves<G: Generate, PL: Side, L: Legality>(&mut self, target: Bitboard) {
        let (rank_7, rank_3): (Bitboard, Bitboard) = if PL::player() == Player::WHITE {
            (Bitboard::RANK_7, Bitboard::RANK_3)
        } else {
            (Bitboard::RANK_2, Bitboard::RANK_6)
        };

        let all_pawns = self.position.piece_bb(PL::player(), PieceType::Pawn);

        // Separated out for promotion moves
        let pawns_rank_7: Bitboard = all_pawns & rank_7;

        // Separated out for non promotion moves
        let pawns_not_rank_7: Bitboard = all_pawns & !rank_7;

        let enemies = self.them_occ;

        // Single and double pawn moves
        let empty_squares = !self.position.occupied();

        if G::kind() == Generation::All || G::kind() == Generation::Quiets {
            let mut push_one = empty_squares & PL::shift_up(pawns_not_rank_7);
            let mut push_two = PL::shift_up(push_one & rank_3) & empty_squares;

            push_one &= target;
            push_two &= target;

            for dest in push_one {
                let orig = PL::down(dest);
                self.add_move::<L>(Move::build(orig, dest, None, MoveType::QUIET));
            }

            for dest in push_two {
                let orig = PL::down(PL::down(dest));
                self.add_move::<L>(Move::build(orig, dest, None, MoveType::QUIET));
            }
        }

        if G::kind() == Generation::All
            || G::kind() == Generation::Promotions
            || G::kind() == Generation::QueenPromotions
        {
            // Promotions
            if pawns_rank_7.is_not_empty() {
                let no_cap_promo = target & PL::shift_up(pawns_rank_7) & empty_squares;
                let left_cap_promo = target & PL::shift_up_left(pawns_rank_7) & enemies;
                let right_cap_promo = target & PL::shift_up_right(pawns_rank_7) & enemies;

                for dest in no_cap_promo {
                    let orig = PL::down(dest);
                    self.add_promo_moves::<G, L>(orig, dest, false);
                }

                for dest in left_cap_promo {
                    let orig = PL::down_right(dest);
                    self.add_promo_moves::<G, L>(orig, dest, true);
                }

                for dest in right_cap_promo {
                    let orig = PL::down_left(dest);
                    self.add_promo_moves::<G, L>(orig, dest, true);
                }
            }
        }

        if G::kind() == Generation::All || G::kind() == Generation::Captures {
            // Captures
            let left_cap = target & PL::shift_up_left(pawns_not_rank_7) & enemies;
            let right_cap = target & PL::shift_up_right(pawns_not_rank_7) & enemies;

            for dest in left_cap {
                let orig = PL::down_right(dest);
                self.add_move::<L>(Move::build(orig, dest, None, MoveType::CAPTURE));
            }

            for dest in right_cap {
                let orig = PL::down_left(dest);
                self.add_move::<L>(Move::build(orig, dest, None, MoveType::CAPTURE));
            }

            if let Some(ep_square) = self.position.ep_square() {
                // TODO: add an `assert_eq` to check that the rank of ep_square is 6th
                // rank from the moving player's perspective

                let ep_cap =
                    pawns_not_rank_7 & Bitboard(pawn_attacks_from(ep_square, PL::opp_player()));

                for orig in ep_cap {
                    self.add_move::<L>(Move::build(
                        orig,
                        ep_square,
                        None,
                        MoveType::EN_PASSANT | MoveType::CAPTURE,
                    ));
                }
            }
        }
    }

    #[inline(always)]
    fn valid_pawn_move<G: Generate, PL: Side, L: Legality>(
        &mut self,
        target: Bitboard,
        orig: Bitboard,
    ) -> bool {
        let (rank_7, rank_3): (Bitboard, Bitboard) = if PL::player() == Player::WHITE {
            (Bitboard::RANK_7, Bitboard::RANK_3)
        } else {
            (Bitboard::RANK_2, Bitboard::RANK_6)
        };

        let pawn = self.position.piece_bb(PL::player(), PieceType::Pawn) & orig;

        // Separated out for promotion moves
        let pawns_rank_7: Bitboard = pawn & rank_7;

        // Separated out for non promotion moves
        let pawns_not_rank_7: Bitboard = pawn & !rank_7;

        let enemies = self.them_occ;

        // Single and double pawn moves
        let empty_squares = !self.position.occupied();

        if G::kind() == Generation::All || G::kind() == Generation::Quiets {
            let mut push_one = empty_squares & PL::shift_up(pawns_not_rank_7);
            let mut push_two = PL::shift_up(push_one & rank_3) & empty_squares;

            push_one &= target;
            push_two &= target;

            if push_one.is_not_empty() {
                return true;
            }

            if push_two.is_not_empty() {
                return true;
            }
        }

        if G::kind() == Generation::All
            || G::kind() == Generation::Promotions
            || G::kind() == Generation::QueenPromotions
        {
            // Promotions
            if pawns_rank_7.is_not_empty() {
                let no_cap_promo = target & PL::shift_up(pawns_rank_7) & empty_squares;
                let left_cap_promo = target & PL::shift_up_left(pawns_rank_7) & enemies;
                let right_cap_promo = target & PL::shift_up_right(pawns_rank_7) & enemies;

                if no_cap_promo.is_not_empty() {
                    return true;
                }

                if left_cap_promo.is_not_empty() {
                    return true;
                }

                if right_cap_promo.is_not_empty() {
                    return true;
                }
            }
        }

        if G::kind() == Generation::All || G::kind() == Generation::Captures {
            // Captures
            let left_cap = target & PL::shift_up_left(pawns_not_rank_7) & enemies;
            let right_cap = target & PL::shift_up_right(pawns_not_rank_7) & enemies;

            if left_cap.is_not_empty() {
                return true;
            }

            if right_cap.is_not_empty() {
                return true;
            }

            if let Some(ep_square) = self.position.ep_square() {
                // TODO: add an `assert_eq` to check that the rank of ep_square is 6th
                // rank from the moving player's perspective

                if (ep_square.to_bb() & target).is_empty() {
                    return false;
                }

                let ep_cap =
                    pawns_not_rank_7 & Bitboard(pawn_attacks_from(ep_square, PL::opp_player()));

                if ep_cap.is_not_empty() {
                    return true;
                }
            }
        }

        false
    }

    // Generates castling for both sides
    #[inline(always)]
    fn generate_castling<PL: Side, L: Legality>(&mut self) {
        self.castling_side::<PL, L>(CastleType::Queenside);
        self.castling_side::<PL, L>(CastleType::Kingside);
    }

    // Generates castling for a single side
    #[inline(always)]
    fn castling_side<PL: Side, L: Legality>(&mut self, side: CastleType) {
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
                let attackers = self.position.attackers_to(s) & enemies;
                if attackers.is_not_empty() {
                    can_castle = false;
                    break 'outer;
                }
                s = direction(s);
            }
            if can_castle {
                self.add_move::<L>(Move::build(ksq, k_to, None, MoveType::CASTLE));
            }
        }
    }

    #[inline(always)]
    fn moves_bb<P: PieceTrait>(&mut self, sq: Square) -> Bitboard {
        debug_assert!(sq.is_okay());
        debug_assert_ne!(P::kind(), PieceType::Pawn);
        match P::kind() {
            PieceType::None => panic!(), // TODO
            PieceType::Pawn => panic!(),
            PieceType::Knight => knight_moves(sq),
            PieceType::Bishop => bishop_moves(self.occ, sq),
            PieceType::Rook => rook_moves(self.occ, sq),
            PieceType::Queen => queen_moves(self.occ, sq),
            PieceType::King => king_moves(sq),
        }
    }

    #[inline(always)]
    fn move_append_from_bb_flag<L: Legality>(
        &mut self,
        bb: &mut Bitboard,
        orig: Square,
        ty: MoveType,
    ) {
        for dest in bb {
            let mov = Move::build(orig, dest, None, ty);
            self.add_move::<L>(mov);
        }
    }

    /// Add promotion moves. This may either only queen promotions, or all four possible promotions
    /// depending on the generation type passed as the `Generate` parameter. If something other
    /// than `All`, `Promotions` or `QueenPromotions` is passed, this function will panic.
    #[inline(always)]
    fn add_promo_moves<G: Generate, L: Legality>(
        &mut self,
        orig: Square,
        dest: Square,
        is_capture: bool,
    ) {
        use Generation::*;
        match G::kind() {
            All | Promotions => self.add_all_promo_moves::<L>(orig, dest, is_capture),
            QueenPromotions => self.add_queen_promo_moves::<L>(orig, dest, is_capture),
            _ => unreachable!(),
        }
    }

    /// Add only queen promotion moves (`=Q`)
    #[inline(always)]
    fn add_queen_promo_moves<L: Legality>(&mut self, orig: Square, dest: Square, is_capture: bool) {
        let move_ty = if is_capture {
            MoveType::PROMOTION | MoveType::CAPTURE
        } else {
            MoveType::PROMOTION
        };
        self.add_move::<L>(Move::build(orig, dest, Some(PieceType::Queen), move_ty));
    }

    /// Add the four possible promo moves (`=N`, `=B`, `=R`, `=Q`)
    #[inline(always)]
    fn add_all_promo_moves<L: Legality>(&mut self, orig: Square, dest: Square, is_capture: bool) {
        let move_ty = if is_capture {
            MoveType::PROMOTION | MoveType::CAPTURE
        } else {
            MoveType::PROMOTION
        };
        for piece in PROMO_PIECES {
            self.add_move::<L>(Move::build(orig, dest, Some(piece), move_ty));
        }
    }

    #[inline(always)]
    fn add_move<L: Legality>(&mut self, mv: Move) {
        if L::legality_type() == LegalityKind::Legal {
            if self.position.legal_move(&mv) {
                self.movelist.push(mv);
            }
        } else {
            self.movelist.push(mv);
        }
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
#[inline(always)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::init_globals;
    use crate::position::Position;

    fn number_of_captures(fen: &str) -> usize {
        let pos = Position::from_fen(fen).unwrap();
        let captures = pos.generate::<BasicMoveList, Captures, Legal>();
        captures.len()
    }

    struct Perft<'a> {
        position: &'a mut Position,
        nodes: usize,
    }

    impl<'a> Perft<'a> {
        pub fn new(position: &'a mut Position) -> Self {
            Self { position, nodes: 0 }
        }

        fn perft(&mut self, depth: usize) {
            if depth == 0 {
                self.nodes += 1;
                return;
            }

            let moves = self.position.generate::<BasicMoveList, Captures, Legal>();

            for mov in &moves {
                self.position.make_move(mov);
                self.perft(depth - 1);
                self.position.unmake_move();
            }
        }
    }

    fn perft_captures_only(fen: &str, depth: usize) -> usize {
        let mut pos = Position::from_fen(fen).unwrap();
        let mut perft = Perft::new(&mut pos);
        perft.perft(depth);
        perft.nodes
    }

    /// Ensure that `generate_captures()` returns the right number of capture moves
    /// for a suite of test positions.
    #[test]
    #[rustfmt::skip]
    fn correct_capture_counts() {
        init_globals();

        assert_eq!(number_of_captures("r1bqk1r1/1p1p1n2/p1n2pN1/2p1b2Q/2P1Pp2/1PN5/PB4PP/R4RK1 w q - 0 1"), 4);
        assert_eq!(number_of_captures("r1n2N1k/2n2K1p/3pp3/5Pp1/b5R1/8/1PPP4/8 w - - 0 1"), 5);
        assert_eq!(number_of_captures("r1b1r1k1/1pqn1pbp/p2pp1p1/P7/1n1NPP1Q/2NBBR2/1PP3PP/R6K w - - 0 1"), 3);
        assert_eq!(number_of_captures("5b2/p2k1p2/P3pP1p/n2pP1p1/1p1P2P1/1P1KBN2/7P/8 w - - 0 1"), 2);
        assert_eq!(number_of_captures("r3kbnr/1b3ppp/pqn5/1pp1P3/3p4/1BN2N2/PP2QPPP/R1BR2K1 w kq - 0 1"), 5);
        assert_eq!(number_of_captures("r2r2k1/1p1n1pp1/4pnp1/8/PpBRqP2/1Q2B1P1/1P5P/R5K1 b - - 0 1"), 4);
        assert_eq!(number_of_captures("2rq1rk1/pb1n1ppN/4p3/1pb5/3P1Pn1/P1N5/1PQ1B1PP/R1B2RK1 b - - 0 1"), 4);
        assert_eq!(number_of_captures("r2qk2r/ppp1bppp/2n5/3p1b2/3P1Bn1/1QN1P3/PP3P1P/R3KBNR w KQkq - 0 1"), 4);
        assert_eq!(number_of_captures("rnb1kb1r/p4p2/1qp1pn2/1p2N2p/2p1P1p1/2N3B1/PPQ1BPPP/3RK2R w Kkq - 0 1"), 7);
        assert_eq!(number_of_captures("5rk1/pp1b4/4pqp1/2Ppb2p/1P2p3/4Q2P/P3BPP1/1R3R1K b - - 0 1"), 1);
        assert_eq!(number_of_captures("r1b2r1k/ppp2ppp/8/4p3/2BPQ3/P3P1K1/1B3PPP/n3q1NR w - - 0 1"), 6);
        assert_eq!(number_of_captures("1nkr1b1r/5p2/1q2p2p/1ppbP1p1/2pP4/2N3B1/1P1QBPPP/R4RK1 w - - 0 1"), 5);
        assert_eq!(number_of_captures("1nrq1rk1/p4pp1/bp2pn1p/3p4/2PP1B2/P1PB2N1/4QPPP/1R2R1K1 w - - 0 1"), 5);
        assert_eq!(number_of_captures("5k2/1rn2p2/3pb1p1/7p/p3PP2/PnNBK2P/3N2P1/1R6 w - - 0 1"), 3);
        assert_eq!(number_of_captures("8/p2p4/r7/1k6/8/pK5Q/P7/b7 w - - 0 1"), 1);
        assert_eq!(number_of_captures("1b1rr1k1/pp1q1pp1/8/NP1p1b1p/1B1Pp1n1/PQR1P1P1/4BP1P/5RK1 w - - 0 1"), 3);
        assert_eq!(number_of_captures("1r3rk1/6p1/p1pb1qPp/3p4/4nPR1/2N4Q/PPP4P/2K1BR2 b - - 0 1"), 6);
        assert_eq!(number_of_captures("r1b1kb1r/1p1n1p2/p3pP1p/q7/3N3p/2N5/P1PQB1PP/1R3R1K b kq - 0 1"), 3);
        assert_eq!(number_of_captures("3kB3/5K2/7p/3p4/3pn3/4NN2/8/1b4B1 w - - 0 1"), 2);
        assert_eq!(number_of_captures("1nrrb1k1/1qn1bppp/pp2p3/3pP3/N2P3P/1P1B1NP1/PBR1QPK1/2R5 w - - 0 1"), 4);
        assert_eq!(number_of_captures("3rr1k1/1pq2b1p/2pp2p1/4bp2/pPPN4/4P1PP/P1QR1PB1/1R4K1 b - - 0 1"), 3);
        assert_eq!(number_of_captures("r4rk1/p2nbpp1/2p2np1/q7/Np1PPB2/8/PPQ1N1PP/1K1R3R w - - 0 1"), 1);
        assert_eq!(number_of_captures("r3r2k/1bq1nppp/p2b4/1pn1p2P/2p1P1QN/2P1N1P1/PPBB1P1R/2KR4 w - - 0 1"), 2);
        assert_eq!(number_of_captures("r2q1r1k/3bppbp/pp1p4/2pPn1Bp/P1P1P2P/2N2P2/1P1Q2P1/R3KB1R w KQ - 0 1"), 1);
        assert_eq!(number_of_captures("2kb4/p7/r1p3p1/p1P2pBp/R2P3P/2K3P1/5P2/8 w - - 0 1"), 2);
        assert_eq!(number_of_captures("rqn2rk1/pp2b2p/2n2pp1/1N2p3/5P1N/1PP1B3/4Q1PP/R4RK1 w - - 0 1"), 5);
        assert_eq!(number_of_captures("8/3Pk1p1/1p2P1K1/1P1Bb3/7p/7P/6P1/8 w - - 0 1"), 0);
        assert_eq!(number_of_captures("4rrk1/Rpp3pp/6q1/2PPn3/4p3/2N5/1P2QPPP/5RK1 w - - 0 1"), 3);
        assert_eq!(number_of_captures("2q2rk1/2p2pb1/PpP1p1pp/2n5/5B1P/3Q2P1/4PPN1/2R3K1 w - - 0 1"), 4);
    }

    #[test]
    fn kiwipete_perft_captures_only() {
        init_globals();

        let res = perft_captures_only(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            8,
        );

        println!("res: {}", res);

        // The result below is for 'captures' as defined in Seaborg. This means moves which are
        // captures but not promotions. Promotion captures are generated as part of the promotion
        // generation phase, and to avoid complexity deduplicating, they are not generated as part
        // of the capture phase.
        assert_eq!(res, 4_224_543);
    }
}
