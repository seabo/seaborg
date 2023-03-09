use crate::bb::Bitboard;
use crate::mono_traits::{
    All, BishopType, BlackType, Captures, Evasions, Generate, KingType,
    KnightType, Legal, Legality, NonEvasions, PieceTrait, Side,
    PseudoLegal, QueenType, QuietChecks, Quiet, RookType, WhiteType,
};
use crate::mov::{Move, MoveType};
use crate::movelist::{BasicMoveList, Frame, MoveList, MoveStack};
use crate::position::{CastleType, PieceType, Player, Position, Square, PROMO_PIECES};
use crate::precalc::boards::{between_bb, king_moves, knight_moves, line_bb, pawn_attacks_from};
use crate::precalc::magic;

use std::ops::Index;

/// Types of move generating options.
///
/// `GenTypes::All` -> All available moves.
///
/// `GenTypes::Captures` -> All captures.
///
/// `GenTypes::Quiets` -> All non captures.
///
/// `GenTypes::QuietChecks` -> Moves likely to give check.
///
/// `GenTypes::Evasions` -> Generates evasions for a board in check.
///
/// `GenTypes::NonEvasions` -> Generates all moves for a board not in check.
///
/// # Safety
///
/// `GenTypes::QuietChecks` and `GenTypes::NonEvasions` can only be used if the board
/// if not in check, while `GenTypes::Evasions` can only be used if the the board is
/// in check. The remaining `GenTypes` can be used legally whenever.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GenType {
    All,
    Captures,
    Quiets,
    QuietChecks,
    Evasions,
    NonEvasions,
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
    /// Generates pseudo-legal moves for the passed position.
    ///
    /// This function could return moves which are either:
    /// - Legal
    /// - Would cause a discovered check (i.e. the moving piece is pinned)
    /// - Would cause the moving king to land in check
    #[inline]
    pub fn generate<L: MoveList>(position: &Position) -> L {
        let mut movelist = L::empty();
        InnerMoveGen::<L>::generate::<All, Legal>(position, &mut movelist);
        movelist
    }

    #[inline]
    pub fn generate_of_legality<ML: MoveList, L: Legality>(position: &Position) -> ML {
        let mut movelist = ML::empty();
        InnerMoveGen::<ML>::generate::<All, L>(position, &mut movelist);
        movelist
    }

    /// Generates moves of type defined by `L: Legality` and pushes them onto the passed
    /// `MoveList`.
    #[inline]
    pub fn generate_in<ML: MoveList, L: Legality>(position: &Position, ms: &mut ML) {
        InnerMoveGen::<ML>::generate::<All, L>(position, ms);
    }

    /// Generates legal moves and pushes them onto the passed `MoveList`.
    #[inline]
    pub fn generate_legal_in<ML: MoveList>(position: &Position, ms: &mut ML) {
        InnerMoveGen::<ML>::generate::<All, Legal>(position, ms);
    }

    #[inline]
    pub fn generate_in_movestack<'a: 'ms + 'p, 'ms, 'p, L: Legality>(
        position: &'p Position,
        ms: &'ms mut MoveStack,
    ) -> Frame<'a> {
        let mut frame = ms.new_frame();
        Self::generate_in::<_, L>(position, &mut frame);
        frame
    }

    #[inline]
    pub fn generate_captures(position: &Position) -> BasicMoveList {
        let mut movelist = BasicMoveList::default();
        InnerMoveGen::<BasicMoveList>::generate::<Captures, Legal>(
            position,
            &mut movelist,
        );
        movelist
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

impl<'a, MP: MoveList> InnerMoveGen<'a, MP>
// where
//     <MP as Index<usize>>::Output: Sized,
{
    /// Generate all pseudo-legal moves in the given position
    #[inline(always)]
    fn generate<G: Generate, L: Legality>(
        position: &'a Position,
        movelist: &'a mut MP,
    ) -> &'a mut MP {
        match position.turn() {
            Player::WHITE => {
                InnerMoveGen::<MP>::generate_helper::<G, L, WhiteType>(position, movelist)
            }
            Player::BLACK => {
                InnerMoveGen::<MP>::generate_helper::<G, L, BlackType>(position, movelist)
            }
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
        let gen_type = G::gen_type();

        if gen_type == GenType::Evasions {
            movegen.generate_evasions::<PL, L>(false);
        } else if gen_type == GenType::Captures {
            if movegen.position.in_check() {
                movegen.generate_evasions::<PL, L>(true);
            } else {
                movegen.generate_captures::<PL, L>();
            }
        }
        // else if gen_type == GenType::Quiets {
        //     if movegen.position.in_check() {
        //         movegen.generate_evasions::<P>()
        //     } else {

        //     }
        // }
        else if gen_type == GenType::All {
            if movegen.position.in_check() {
                movegen.generate_evasions::<PL, L>(false);
            } else {
                movegen.generate_all::<PL, L>();
            }
        }

        movegen.movelist
    }

    #[inline(always)]
    fn generate_all<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<P, L>(Bitboard::ALL);
        self.generate_castling::<P, L>();
        self.moves_per_piece::<P, KnightType, L>(Bitboard::ALL);
        self.moves_per_piece::<P, KingType, L>(Bitboard::ALL);
        self.moves_per_piece::<P, RookType, L>(Bitboard::ALL);
        self.moves_per_piece::<P, BishopType, L>(Bitboard::ALL);
        self.moves_per_piece::<P, QueenType, L>(Bitboard::ALL);
    }

    #[inline(always)]
    fn generate_captures<P: Side, L: Legality>(&mut self) {
        self.generate_pawn_moves::<P, L>(self.them_occ);
        self.moves_per_piece::<P, KnightType, L>(self.them_occ);
        self.moves_per_piece::<P, KingType, L>(self.them_occ);
        self.moves_per_piece::<P, RookType, L>(self.them_occ);
        self.moves_per_piece::<P, BishopType, L>(self.them_occ);
        self.moves_per_piece::<P, QueenType, L>(self.them_occ);
    }

    #[inline(always)]
    fn generate_evasions<P: Side, L: Legality>(&mut self, captures_only: bool) {
        debug_assert!(self.position.in_check());

        let target_sqs = if captures_only {
            self.them_occ
        } else {
            Bitboard::ALL
        };

        let ksq = self.position.king_sq(P::player());
        let mut slider_attacks = Bitboard(0);

        // Pieces that could possibly attack the king with sliding attacks
        let mut sliders = self.position.checkers()
            & !self
                .position
                .piece_two_bb_both_players(PieceType::Pawn, PieceType::Knight);

        // All the squares that are attacked by sliders
        // TODO[movegen]: make this an iterator - we are doing lots of checks in the method
        // `pop_some_lsb_and_bit`. It also is potentially inefficient in creating a new bitboard
        // with `sq.to_bb()`. We should use the bit twiddle `bb & -bb` to isolate the LSB as a bb.
        while let Some((check_sq, check_sq_bb)) = sliders.pop_some_lsb_and_bit() {
            slider_attacks |= Bitboard(line_bb(check_sq, ksq)) ^ check_sq_bb;
        }

        // Possible king moves, where the king cannot move into a slider / own pieces
        let k_moves = king_moves(ksq) & !slider_attacks & !self.us_occ & target_sqs;

        // Separate captures and non-captures
        let mut captures_bb = k_moves & self.them_occ;
        let mut non_captures_bb = k_moves & !self.them_occ;
        self.move_append_from_bb_flag::<L>(&mut captures_bb, ksq, MoveType::CAPTURE);
        self.move_append_from_bb_flag::<L>(&mut non_captures_bb, ksq, MoveType::QUIET);

        // If there is only one checking square, we can block or capture the piece
        if !(self.position.checkers().more_than_one()) {
            let checking_sq = Square(self.position.checkers().bsf() as u8);

            // Squares that allow a block or captures of the sliding piece
            let target =
                target_sqs & (Bitboard(between_bb(checking_sq, ksq)) | checking_sq.to_bb());
            self.generate_pawn_moves::<P, L>(target);
            self.moves_per_piece::<P, KnightType, L>(target);
            self.moves_per_piece::<P, BishopType, L>(target);
            self.moves_per_piece::<P, RookType, L>(target);
            self.moves_per_piece::<P, QueenType, L>(target);
        }
    }

    #[inline(always)]
    fn moves_per_piece<PL: Side, P: PieceTrait, L: Legality>(
        &mut self,
        target: Bitboard,
    ) {
        let piece_bb: Bitboard = self.position.piece_bb(PL::player(), P::piece_type());
        for orig in piece_bb {
            let moves_bb: Bitboard = self.moves_bb::<P>(orig) & !self.us_occ & target;
            let mut captures_bb: Bitboard = moves_bb & self.them_occ;
            let mut non_captures_bb: Bitboard = moves_bb & !self.them_occ;
            self.move_append_from_bb_flag::<L>(&mut captures_bb, orig, MoveType::CAPTURE);
            self.move_append_from_bb_flag::<L>(&mut non_captures_bb, orig, MoveType::QUIET);
        }
    }

    #[inline(always)]
    fn generate_pawn_moves<PL: Side, L: Legality>(&mut self, target: Bitboard) {
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

        // Promotions
        if pawns_rank_7.is_not_empty() {
            let no_cap_promo = target & PL::shift_up(pawns_rank_7) & empty_squares;
            let left_cap_promo = target & PL::shift_up_left(pawns_rank_7) & enemies;
            let right_cap_promo = target & PL::shift_up_right(pawns_rank_7) & enemies;

            for dest in no_cap_promo {
                let orig = PL::down(dest);
                self.add_all_promo_moves::<L>(orig, dest, false);
            }

            for dest in left_cap_promo {
                let orig = PL::down_right(dest);
                self.add_all_promo_moves::<L>(orig, dest, true);
            }

            for dest in right_cap_promo {
                let orig = PL::down_left(dest);
                self.add_all_promo_moves::<L>(orig, dest, true);
            }
        }

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

    /// Add the four possible promo moves (`=N`, `=B`, `=R`, `=Q`)
    #[inline(always)]
    fn add_all_promo_moves<L: Legality>(
        &mut self,
        orig: Square,
        dest: Square,
        is_capture: bool,
    ) {
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
    use crate::init::init_globals;
    use crate::position::Position;

    fn number_of_captures(fen: &str) -> usize {
        let pos = Position::from_fen(fen).unwrap();
        let captures = pos.generate_captures();
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
            }

            let moves = self.position.generate_captures();
            // let all_moves = self.position.generate_moves();

            // use crate::movelist::{MVPushable, MoveList};
            // let mut caps = MoveList::default();
            // for mov in &all_moves {
            //     if mov.is_capture() {
            //         caps.push(*mov);
            //     }
            // }

            // if caps.len() != moves.len() {
            //     println!("-----");
            //     print!("Hist: ");
            //     for mov in self.position.history() {
            //         print!("{} ", mov);
            //     }
            //     print!("\n");
            //     println!("Caps:\n {}", caps);
            //     println!("Moves:\n {}", moves);
            // }

            for mov in &moves {
                if depth == 1 {
                    self.nodes += 1;
                } else {
                    self.position.make_move(*mov);
                    self.perft(depth - 1);
                    self.position.unmake_move();
                }
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
    #[rustfmt::skip]
    #[test]
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

        assert_eq!(
            perft_captures_only(
                "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
                8
            ),
            5_068_953
        );
    }
}
