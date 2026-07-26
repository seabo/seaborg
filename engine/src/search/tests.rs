use super::*;
use crate::ordering::{Phase, ORDERING_BUFFER_CAPACITY};
use chess::mov::MoveType;
use chess::position::Square;
use std::time::Duration;

#[rustfmt::skip]
fn suite() -> Vec<(&'static str, u8, Score, Score, &'static [&'static str])> {
    // Test position tuples have the form:
    // (fen, depth, score range, acceptable_best_moves)
    //
    // The final field lists every move that is objectively optimal for the
    // position. Most positions have a single best move, so the slice holds
    // one entry; positions with more than one equally best move list them
    // all, and the search passes if it plays any of them. Pinning a single
    // move for a position that has several would reject a correct answer
    // whenever move ordering happens to surface a different optimal move.

    vec![
            // Mates
            ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6, Score::mate(5), Score::mate(5), &["c7c6"]),
            ("5R2/1p1r2pk/p1n1B2p/2P1q3/2Pp4/P6b/1B1P4/2K3R1 w - - 5 3", 6, Score::mate(5), Score::mate(5), &["e6g8"]),
            ("1r6/p5pk/1q1p2pp/3P3P/4Q1P1/3p4/PP6/3KR3 w - - 0 36", 6, Score::mate(5), Score::mate(5), &["h5g6"]),
            ("1r4k1/p3p1bp/5P1r/3p2Q1/5R2/3Bq3/P1P2RP1/6K1 b - - 0 33", 6, Score::mate(5), Score::mate(5), &["b8b1"]),
            // Searched a ply deeper than its siblings: late-move reduction defers proving this
            // forced mate by one iteration, so the mate score surfaces at depth 7 rather than 6.
            // The best move d3d7 is already found at depth 6; only the exact mate distance lags.
            ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 7, Score::mate(5), Score::mate(5), &["d3d7"]),
            ("5rk1/rb3ppp/p7/1pn1q3/8/1BP2Q2/PP3PPP/3R1RK1 w - - 7 21", 6, Score::mate(5), Score::mate(5), &["f3f7"]),
            ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 8, Score::mate(7), Score::mate(7), &["g4g2"]),
            ("6k1/1p2qppp/4p3/8/p2PN3/P5QP/1r4PK/8 w - - 0 40", 6, Score::mate(5), Score::mate(5), &["e4f6"]),
            ("2R1bk2/p5pp/5p2/8/3n4/3p1B1P/PP1q1PP1/4R1K1 w - - 0 27", 6, Score::mate(5), Score::mate(5), &["c8e8"]),
            ("8/7R/r4pr1/5pkp/1R6/P5P1/5PK1/8 w - - 0 42", 6, Score::mate(5), Score::mate(5), &["h7h5"]),
            ("r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25", 8, Score::mate(7), Score::mate(7), &["h5f7"]),

            // // Winning material
            ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 7, Score::cp(345), Score::cp(385), &["g7g6"]),
            ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, Score::cp(40), Score::cp(90), &["g1e1"]),
            ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 3, Score::cp(850), Score::cp(950), &["d3c4"]),
            // Wider upper bound than the raw material count: the check-evasion extension searches
            // this promoting rook-endgame position deeper, so the depth-6 score reads a little
            // higher than the un-extended search once did. The best move d2d8 is unchanged.
            ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6, Score::cp(905), Score::cp(985), &["d2d8"]),
            ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 4, Score::cp(330), Score::cp(370), &["d1b3"]),
            ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 5, Score::cp(408), Score::cp(448), &["g7h6"]),
            ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 6, Score::cp(0), Score::cp(0), &["c7h7"]),

            // Pawn race. Ka1 has exactly two winning moves, Kb1 and Kb2 (both
            // WIN by the Syzygy KPvKP tablebase); the pawn pushes throw the
            // win away (a2a4 draws, a2a3 loses), so the king must step aside
            // first. The two king moves are equally optimal, and which one the
            // search returns depends on quiet-move ordering, so both are
            // accepted. Searched two plies deeper than the winning move needs:
            // the transposition-miss depth reduction speculatively shortens the
            // long, move-less king-and-pawn lines, so the winning score surfaces
            // at depth 24 rather than 22. The best move is found well before
            // either — only the promotion's full value lags behind the horizon.
            ("8/6pk/8/8/8/8/P7/K7 w - - 0 1", 24, Score::cp(450), Score::cp(920), &["a1b1", "a1b2"]),
    ]
}

/// Razoring relies on a static centipawn evaluation, so mate and infinity bounds are excluded.
#[test]
fn razoring_only_applies_to_centipawn_bounds() {
    assert!(should_razor(1, Score::cp(-1_000), Score::cp(0), false));
    assert!(!should_razor(1, Score::cp(-1_000), Score::mate(5), false));
    assert!(!should_razor(1, Score::cp(-1_000), Score::INF_P, false));
}

/// The improving signal widens the razoring margin, so a node that would be razored when the
/// side is stagnating survives when it is improving. A deficit is chosen that sits between the
/// two margins: below the base margin it razors, but the extra [`RAZOR_IMPROVING_MARGIN`] lifts
/// `eval + margin` back above alpha when improving.
#[test]
fn razoring_is_more_reluctant_when_improving() {
    let alpha = Score::cp(0);
    // Base depth-1 margin is 426 + 252 = 678. A 700cp deficit clears it, razoring, but not the
    // 678 + RAZOR_IMPROVING_MARGIN widened margin.
    let eval = Score::cp(-700);
    assert!(should_razor(1, eval, alpha, false));
    assert!(!should_razor(1, eval, alpha, true));

    // A deficit past even the widened margin razors regardless of the trend.
    let deep = Score::cp(-2_000);
    assert!(should_razor(1, deep, alpha, false));
    assert!(should_razor(1, deep, alpha, true));
}

/// The improving signal is true exactly when this ply's static evaluation exceeds the same
/// side's evaluation two plies earlier, and false whenever either value is missing. Following
/// a per-side evaluation that rises and then falls, the signal is true through the ascent and
/// false through the descent, and it is false at the first two plies, which have no ancestor to
/// compare against.
#[test]
fn improving_tracks_a_rising_then_falling_evaluation() {
    // Static evaluation by ply. Even plies (one side) climb 10, 20, 30 then fall to 15; odd
    // plies (the other side) climb 5, 15 then fall to 8, 3.
    let evals = [
        Some(Score::cp(10)), // ply 0
        Some(Score::cp(5)),  // ply 1
        Some(Score::cp(20)), // ply 2  vs ply 0: 20 > 10 -> improving
        Some(Score::cp(15)), // ply 3  vs ply 1: 15 > 5  -> improving
        Some(Score::cp(30)), // ply 4  vs ply 2: 30 > 20 -> improving
        Some(Score::cp(8)),  // ply 5  vs ply 3: 8  < 15 -> not
        Some(Score::cp(15)), // ply 6  vs ply 4: 15 < 30 -> not
        Some(Score::cp(3)),  // ply 7  vs ply 5: 3  < 8  -> not
    ];
    let expected = [false, false, true, true, true, false, false, false];

    for ply in 0..evals.len() {
        let two_back = ply.checked_sub(2).and_then(|p| evals[p]);
        assert_eq!(
            is_improving(evals[ply], two_back),
            expected[ply],
            "improving mismatched at ply {ply}"
        );
    }

    // A node in check computes no evaluation, so the signal is false whether the missing value
    // is the current ply or the earlier one.
    assert!(!is_improving(None, Some(Score::cp(10))));
    assert!(!is_improving(Some(Score::cp(10)), None));
    assert!(!is_improving(None, None));
}

/// The base reduction table must grow with both remaining depth and move count and never shrink
/// in either: a deeper or later move is the one least likely to repay a full search, so it may
/// never be reduced less than a shallower or earlier one. Monotonicity is what makes the reduction
/// a coherent "how far down the ordering" signal rather than a bag of tuned points.
#[test]
fn lmr_base_table_grows_monotonically_in_depth_and_move_count() {
    // Non-decreasing along move count at every depth.
    for depth in 1..64 as Depth {
        for move_count in 1..200u8 {
            assert!(
                LMR_TABLE.base(depth, move_count) >= LMR_TABLE.base(depth, move_count - 1),
                "reduction fell as move count rose at depth {depth}, move {move_count}"
            );
        }
    }
    // Non-decreasing along depth at every move count.
    for depth in 2..64 as Depth {
        for move_count in 1..200u8 {
            assert!(
                LMR_TABLE.base(depth, move_count) >= LMR_TABLE.base(depth - 1, move_count),
                "reduction fell as depth rose at depth {depth}, move {move_count}"
            );
        }
    }
    // And it genuinely grows across the range rather than being flat.
    assert!(
        LMR_TABLE.base(32, 32) > LMR_TABLE.base(3, 4),
        "the table did not grow from an early shallow move to a late deep one"
    );
}

/// A move the ordering tables already trust is reduced less, and one they distrust more: the
/// reduction eases as accumulated quiet history rises. This is the whole point of spending the
/// history signal on the reduction amount.
#[test]
fn lmr_eases_with_strong_history_and_deepens_with_weak() {
    // A depth and move count whose base reduction has headroom in both directions.
    let strong = lmr_reduction(16, 16, false, true, false, 80_000);
    let neutral = lmr_reduction(16, 16, false, true, false, 0);
    let weak = lmr_reduction(16, 16, false, true, false, -80_000);
    assert!(
        strong < neutral && neutral < weak,
        "history did not modulate the reduction: strong {strong}, neutral {neutral}, weak {weak}"
    );
}

/// A side to move that is not improving takes exactly one extra ply of reduction; an improving
/// side keeps the base. The deteriorating side's moves matter less and can be trimmed harder.
#[test]
fn lmr_non_improving_reduces_one_extra_ply() {
    let improving = lmr_reduction(16, 16, false, true, false, 0);
    let not_improving = lmr_reduction(16, 16, false, false, false, 0);
    assert_eq!(
        not_improving,
        improving + 1,
        "a non-improving side should take exactly one extra ply of reduction"
    );
}

/// PV nodes and killer/counter moves are reduced less than a plain late quiet at the same depth
/// and move count, so the trusted ordering prefix keeps its depth.
#[test]
fn lmr_favours_pv_nodes_and_ordering_refutations() {
    let plain = lmr_reduction(16, 16, false, true, false, 0);
    let pv = lmr_reduction(16, 16, true, true, false, 0);
    let favoured = lmr_reduction(16, 16, false, true, true, 0);
    assert!(
        pv < plain,
        "a PV node should be reduced less: pv {pv}, plain {plain}"
    );
    assert!(
        favoured < plain,
        "a killer/counter move should be reduced less: favoured {favoured}, plain {plain}"
    );
}

/// The reduction never drops below zero even when every easing signal fires at once: a modulation
/// sum below zero means "do not reduce", never "extend" — late-move extensions are out of scope.
#[test]
fn lmr_never_returns_a_negative_reduction() {
    let r = lmr_reduction(3, 4, true, true, true, HISTORY_MAX * 3);
    assert!(r >= 0, "reduction went negative: {r}");
}

#[test]
fn trained_quiets_are_ordered_without_narrowing_history_scores() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let generated = position.generate::<BasicMoveList, Quiets, Legal>();
    let poor = generated[0];
    let good = generated[1];
    let side = position.turn();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    search
        .history
        .update(poor.orig(), poor.dest(), -HISTORY_MAX, side);
    search
        .history
        .update(good.orig(), good.dest(), HISTORY_MAX, side);
    assert!(search.history.get(good.orig(), good.dest(), side) > i16::MAX.into());

    let mut ordered = OrderedMoves::new();
    while ordered.load_next_phase(MoveLoader::from(&mut search, None, 0)) {
        if ordered.phase() == Phase::Quiet {
            let quiets: Vec<Move> = (&mut ordered).into_iter().collect();
            let good_index = quiets.iter().position(|mov| *mov == good).unwrap();
            let poor_index = quiets.iter().position(|mov| *mov == poor).unwrap();
            assert!(good_index < poor_index);
            return;
        }
        for _ in &mut ordered {}
    }

    panic!("quiet phase was not loaded");
}

#[test]
fn history_bonus_grows_with_depth_and_gravity_applies_malus() {
    let from = Square::A2;
    let to = Square::A3;
    let side = Player::WHITE;
    let mut shallow = HistoryTable::new();
    let mut deep = HistoryTable::new();

    shallow.update(from, to, history_bonus(2), side);
    deep.update(from, to, history_bonus(8), side);
    assert!(deep.get(from, to, side) > shallow.get(from, to, side));

    let before = deep.get(from, to, side);
    deep.update(from, to, -history_bonus(8), side);
    assert!(deep.get(from, to, side) < before);
}

/// Drive staged ordering with a real [`MoveLoader`] at `ply` and return what each phase yields,
/// so tests can observe the combined contextual quiet order and the counter stage directly.
fn ordered_phases(search: &mut Search<'_>, ply: usize) -> Vec<(Phase, Vec<Move>)> {
    let mut moves = OrderedMoves::new();
    let mut out = Vec::new();
    while moves.load_next_phase(MoveLoader::from(search, None, ply)) {
        let phase = moves.phase();
        out.push((phase, (&mut moves).into_iter().collect()));
    }
    out
}

fn phase_yield(phases: &[(Phase, Vec<Move>)], wanted: Phase) -> Vec<Move> {
    phases
        .iter()
        .find(|(phase, _)| *phase == wanted)
        .map(|(_, moves)| moves.clone())
        .unwrap_or_default()
}

/// Load every ordering phase for `fen` with a real [`MoveLoader`] and return the buffer occupancy
/// once all phases are in place, i.e. the number of entries the fixed-capacity buffer had to hold.
fn ordering_buffer_occupancy(fen: &str) -> usize {
    chess::init::init_globals();
    let position = Position::from_fen(fen).unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    let mut moves = OrderedMoves::new();
    while moves.load_next_phase(MoveLoader::from(&mut search, None, 0)) {}
    moves.buffer_occupancy()
}

/// The ordering buffer is a fixed-capacity array, and `ScoredMoveList::push` silently drops a move
/// once it is full. Its capacity rests on an arithmetic argument — worst-case occupancy is about
/// `L + 3P + 3` for `L` legal moves and `P` queen promotions — that nothing enforces at runtime
/// outside a debug build. In real positions the two terms cannot both be large at once: a high
/// promotion count needs pawns on the seventh rank, which displaces the sliding-piece mobility that
/// drives a high legal-move count. This pins two points that bound the argument:
///
/// - the maximum legal mobility (218 moves, no promotions), the `L`-dominated extreme; and
/// - a synthetic position that deliberately breaks the mutual exclusivity — a full seventh-rank
///   promotion wall *and* eight queens for mobility — so both terms are large together. It is
///   unreachable in a real game but legal, and gives a conservative worst case above either
///   realistic extreme alone.
///
/// A later phase addition that loads more moves changes these counts and fails the test here,
/// rather than silently truncating a move list somewhere in the search.
#[test]
fn ordering_buffer_worst_case_occupancy_stays_within_capacity() {
    let max_mobility = "R6R/3Q4/1Q4Q1/4Q3/2Q4Q/Q4Q2/pp1Q4/kBNN1KB1 w - - 0 1";
    let promotion_and_mobility = "nnnnnnnn/PPPPPPPP/Q6Q/2Q2Q2/2Q2Q2/Q6Q/8/K6k w - - 0 1";

    let max_occupancy = ordering_buffer_occupancy(max_mobility);
    let stress_occupancy = ordering_buffer_occupancy(promotion_and_mobility);

    // Exact measured occupancy. A change here means a phase now loads a different number of moves;
    // confirm the capacity argument still holds before updating these constants.
    assert_eq!(max_occupancy, 218, "maximum-mobility occupancy changed");
    assert_eq!(
        stress_occupancy, 156,
        "promotion-and-mobility occupancy changed"
    );

    assert!(
        max_occupancy < ORDERING_BUFFER_CAPACITY && stress_occupancy < ORDERING_BUFFER_CAPACITY,
        "measured occupancy must stay within the ordering buffer capacity",
    );
}

/// Continuation history conditions a quiet on the preceding move, so a move that is a strong
/// reply to what was just played is ordered ahead of one with more plain from-to history but no
/// such contextual evidence. This is the whole point of the table: plain history alone cannot
/// distinguish a generally useful move from a specifically good reply.
#[test]
fn continuation_history_orders_a_reply_ahead_of_a_higher_plain_history_move() {
    chess::init::init_globals();
    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    // The node at ply 1 was reached by a black pawn move to e5; that is the one-ply continuation
    // context every quiet here is scored against.
    let ply = 1;
    search.stack[0].mov = Move::build(Square::E7, Square::E5, None, MoveType::QUIET);
    search.stack[0].moved_piece = Piece::BlackPawn;

    // g1f3 has no plain history but strong continuation evidence as a reply to ...e5; e2e4 has
    // stronger plain history and no continuation evidence. The tables are keyed on the piece each
    // move's origin square carries in the start position.
    let reply = Move::build(Square::G1, Square::F3, None, MoveType::QUIET);
    let plain = Move::build(Square::E2, Square::E4, None, MoveType::QUIET);
    search
        .history
        .update(plain.orig(), plain.dest(), 5_000, Player::WHITE);
    search.cont_hist.update(
        0,
        Piece::BlackPawn,
        Square::E5,
        Piece::WhiteKnight,
        Square::F3,
        10_000,
    );

    let quiets = phase_yield(&ordered_phases(&mut search, ply), Phase::Quiet);
    let reply_at = quiets.iter().position(|m| *m == reply);
    let plain_at = quiets.iter().position(|m| *m == plain);
    assert!(
        reply_at < plain_at,
        "the continuation reply should precede the higher-plain-history move: {quiets:?}"
    );
}

/// Among captures with identical static exchange value, the one the search has already found to
/// cause cutoffs is tried first. Static exchange evaluation alone leaves such captures in
/// generation order; capture history is the signal that separates them, and it does so strictly
/// within the phase — here the good-capture phase — so material outcome still decides the order
/// between captures of different value.
#[test]
fn trained_captures_break_ties_among_equal_static_exchange_value() {
    chess::init::init_globals();

    // The white pawn on b4 can capture either undefended black pawn, on a5 or c5. Each wins
    // exactly a pawn, so both land in the good-capture phase with identical static exchange
    // value and nothing but learned history can separate them.
    let position = Position::from_fen("4k3/8/8/p1p5/1P6/8/8/4K3 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    // Reward the c5 capture and penalise the a5 capture. Both are pawn-takes-pawn, keyed apart
    // only by destination square, so history alone should now order c5 ahead of a5 — the reverse
    // of the a-file-first generation order.
    search
        .capture_history
        .update(Piece::WhitePawn, Square::C5, PieceType::Pawn, HISTORY_MAX);
    search
        .capture_history
        .update(Piece::WhitePawn, Square::A5, PieceType::Pawn, -HISTORY_MAX);

    let good = phase_yield(&ordered_phases(&mut search, 0), Phase::GoodCaptures);
    let a5_at = good
        .iter()
        .position(|m| m.dest() == Square::A5)
        .expect("a5 capture should be a good capture");
    let c5_at = good
        .iter()
        .position(|m| m.dest() == Square::C5)
        .expect("c5 capture should be a good capture");
    assert!(
        c5_at < a5_at,
        "the capture with cutoff history should be ordered first: {good:?}"
    );
}

/// A counter move is stored against a preceding move but probed at a possibly different
/// position, so it must be legality-validated before it can be handed to the unsafe move loop.
/// An illegal stored counter is silently dropped; a legal one is yielded by the counter stage.
#[test]
fn a_stored_counter_is_legality_validated_before_it_is_yielded() {
    chess::init::init_globals();
    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    let ply = 1;
    search.stack[0].mov = Move::build(Square::E7, Square::E5, None, MoveType::QUIET);
    search.stack[0].moved_piece = Piece::BlackPawn;

    // A rook slide blocked by its own pawn: impossible in the start position.
    let illegal = Move::build(Square::A1, Square::A5, None, MoveType::QUIET);
    search.counter.store(Piece::BlackPawn, Square::E5, illegal);
    let counter = phase_yield(&ordered_phases(&mut search, ply), Phase::Counter);
    assert!(
        counter.is_empty(),
        "an illegal stored counter must not reach the move loop: {counter:?}"
    );

    // A legal quiet in the start position is validated and yielded by the counter stage.
    let legal = Move::build(Square::G1, Square::F3, None, MoveType::QUIET);
    search.counter.store(Piece::BlackPawn, Square::E5, legal);
    let counter = phase_yield(&ordered_phases(&mut search, ply), Phase::Counter);
    assert_eq!(counter, vec![legal]);
}

#[test]
fn fifty_move_rule_uses_halfmove_boundary() {
    chess::init::init_globals();

    for (halfmove_clock, expected) in [(99, false), (100, true), (101, true)] {
        let fen = format!("4k3/8/8/8/8/8/P7/Q3K3 w - - {halfmove_clock} 1");
        let pos = Position::from_fen(&fen).unwrap();
        assert_eq!(pos.fifty_move_rule_reached(), expected);

        let flag = AtomicBool::new(false);
        let tt = Table::new(1);
        let mut search = Search::new(pos, &flag, None, &tt);
        let result = search.run::<Master>(1).unwrap();
        assert_eq!(result.score == Score::zero(), expected);
    }
}

#[test]
fn quiescence_searches_quiet_check_evasions() {
    chess::init::init_globals();

    let position = Position::from_fen("k3r3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    let score = search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0);

    // White is in check from the rook and has only quiet king moves to escape with, so the
    // returned value is the static evaluation of the best evasion: a rook down, plus the small
    // piece-square difference the king move makes. The exact figure is incidental; the point is
    // that a position with no captures or checks to make is still scored below equality, which
    // can only happen if the quiet evasions were searched at all.
    assert_eq!(score, Some(Score::cp(-449)));
    assert!(search.trace.q_nodes_visited() > 1);
}

#[test]
fn quiescence_detects_checkmate_at_the_horizon() {
    chess::init::init_globals();

    let position = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    assert_eq!(
        search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0),
        Some(Score::mate(0))
    );
}

#[test]
fn quiescence_abort_with_legal_evasions_is_not_checkmate() {
    chess::init::init_globals();

    let position = Position::from_fen("k3r3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    let moves = position.generate::<BasicMoveList, AllGen, Legal>();
    assert!(!moves.is_empty());

    let flag = AtomicBool::new(true);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);
    // Cancellation is only honored once a legal root fallback exists, which `run` establishes
    // before any node is searched. Emulate that armed state so the flag actually stops the
    // search.
    search.root_fallback_ready = true;

    assert_eq!(
        search.quiesce_evasions::<Master, Pv>(Score::INF_N, Score::INF_P, 0, &moves, 0),
        None
    );
}

#[test]
fn quiescence_uses_tt_scores_only_with_valid_bound_semantics() {
    chess::init::init_globals();

    let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);

    for (bound, stored, expected) in [
        (Bound::Exact, Score::cp(12), Score::cp(12)),
        (Bound::Lower, Score::cp(70), Score::cp(70)),
        (Bound::Upper, Score::cp(-70), Score::cp(-70)),
    ] {
        let table = Table::new(1);
        table.store(position.zobrist().0, stored, None, 0, bound, &Move::null());
        let mut search = Search::new(position.clone(), &flag, None, &table);

        assert_eq!(
            search.quiesce::<Master, NonPv>(Score::cp(-50), Score::cp(-49), 0),
            Some(expected)
        );
    }
}

/// A small deterministic NNUE network with a spread of weights kept within the accumulator's
/// i16 domain, for exercising the selectable evaluation seam.
fn test_network() -> Network {
    let hidden = 16u32;
    let h = hidden as usize;
    let mut w_ft = vec![0i16; 768 * h];
    for (feature, column) in w_ft.chunks_mut(h).enumerate() {
        for (unit, w) in column.iter_mut().enumerate() {
            *w = ((feature * 31 + unit * 7) % 41) as i16 - 20;
        }
    }
    let b_ft: Vec<i16> = (0..h).map(|u| (u as i16 % 7) - 3).collect();
    let w_out: Vec<i16> = (0..2 * h).map(|j| ((j * 13) % 49) as i16 - 24).collect();
    Network::new(
        hidden,
        255,
        64,
        400,
        nnue::Parameters {
            w_ft,
            b_ft,
            w_out,
            b_out: vec![0],
        },
    )
    .expect("test network satisfies the build invariant")
}

/// The evaluation is selectable at the single consumption point `Search::evaluate`: setting a
/// network scores leaves through the scalar quantized forward pass, and leaving it unset keeps
/// the hand-crafted tapered evaluation. Selecting the network changes only what `evaluate`
/// returns; clearing it restores the hand-crafted value exactly, so the default path is
/// undisturbed.
#[test]
fn evaluate_selects_the_nnue_forward_pass_when_a_network_is_set() {
    chess::init::init_globals();
    let net = test_network();

    for fen in [
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1",
    ] {
        let pos = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);
        let tt = Table::new(1);
        let mut search = Search::new(pos.clone(), &flag, None, &tt);

        // Default: the hand-crafted tapered evaluation, from the side to move.
        let handcrafted = search.evaluate();
        assert_eq!(handcrafted, Score::cp(pos.static_eval() * search.pov()));

        // Selected: the scalar quantized forward pass, computed independently here and
        // returned already from the side to move's perspective (no `pov` flip).
        search.set_network(Some(Arc::new(net.clone())));
        let acc = Accumulator::from_position(&net, &pos);
        let expected = Score::cp(nnue::forward(&net, &acc, pos.turn()) as i16);
        assert_eq!(
            search.evaluate(),
            expected,
            "NNUE path not selected on {fen}"
        );
        assert_ne!(
            search.evaluate(),
            handcrafted,
            "the two evaluations should differ so the selection is observable on {fen}"
        );

        // Clearing the network restores the hand-crafted path exactly.
        search.set_network(None);
        assert_eq!(search.evaluate(), handcrafted);
    }
}

/// A network set on the `SearchEngine` reaches the searches it starts: this is the plumbing the
/// UCI `EvalFile` option and datagen `--network` both drive. A depth-1 search backs up its
/// root move's leaf value, so selecting a network that scores leaves differently must change the
/// reported score, and clearing it must restore the hand-crafted result.
#[test]
fn search_engine_starts_searches_with_the_configured_network() {
    chess::init::init_globals();
    let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    let pos = Position::from_fen(fen).unwrap();

    let score_of = |engine: &SearchEngine| {
        engine
            .start(pos.clone(), SearchLimit::Depth(1))
            .wait()
            .result()
            .expect("a depth-1 search on a non-terminal position returns a result")
            .score
    };

    let mut engine = SearchEngine::new(1);
    // A fresh engine starts on whatever this build embeds, so the hand-crafted baseline this
    // test compares against has to be asked for.
    assert_eq!(
        engine.network().map(|n| n.param_hash()),
        nnue::built_in_network().map(|n| n.param_hash()),
        "a fresh engine must evaluate with the built-in network"
    );
    engine.set_network(None);
    engine.new_game();
    let handcrafted = score_of(&engine);

    // Clear the shared table alongside each evaluator change, as the driver does: the table
    // caches evaluation-function-dependent static evals that a new evaluator would invalidate.
    engine.set_network(Some(Arc::new(test_network())));
    engine.new_game();
    assert_ne!(
        score_of(&engine),
        handcrafted,
        "the configured network did not reach the started search"
    );

    engine.set_network(None);
    engine.new_game();
    assert_eq!(
        score_of(&engine),
        handcrafted,
        "clearing the network did not restore the hand-crafted search"
    );
}

/// The evaluation must not depend on the halfmove clock, which the Zobrist key does not cover;
/// a leaf value that read it could be computed under one clock and then silently reused under a
/// materially different one. The evaluation once scaled material towards zero as the clock
/// advanced, so this position — a white queen against a bare king — evaluated differently at
/// each clock. It must now score identically at every clock.
///
/// The value is the tapered blend of the middlegame and endgame tables. With only a queen left
/// the game phase is 4 of 24, so the score is (1024 * 4 + 903 * 20) / 24 = 923: not a round
/// material figure, precisely because the piece-square terms are folded in.
#[test]
fn static_evaluation_is_independent_of_the_halfmove_clock() {
    chess::init::init_globals();

    let eval_at = |halfmove_clock: u32| {
        let fen = format!("4k3/8/8/8/8/8/8/Q3K3 w - - {halfmove_clock} 1");
        let pos = Position::from_fen(&fen).unwrap();
        let flag = AtomicBool::new(false);
        let tt = Table::new(1);
        let mut search = Search::new(pos, &flag, None, &tt);
        search.evaluate()
    };

    for halfmove_clock in [0, 50, 99] {
        assert_eq!(
            eval_at(halfmove_clock),
            Score::cp(923),
            "evaluation moved at halfmove clock {halfmove_clock}"
        );
    }
}

/// The evaluation is a pure function of piece placement and colour, so a position and its
/// colour-and-rank mirror must receive equal and opposite scores. This is the check that the
/// piece-square tables are oriented correctly: reading White's table for a Black piece, or
/// forgetting to flip the square, would break here even where a single position still looked
/// plausible.
#[test]
fn the_evaluation_is_symmetric_under_a_colour_mirror() {
    chess::init::init_globals();

    for fen in [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        "4k3/8/8/8/8/8/8/Q3K3 w - - 0 1",
        "r3k2r/pp3ppp/2n5/8/3P4/2N2N2/PP3PPP/R3K2R w - - 0 1",
        "8/2k5/8/4N3/2B5/8/5K2/8 w - - 0 1",
    ] {
        let pos = Position::from_fen(fen).unwrap();
        let mirror = Position::from_fen(&colour_mirror_fen(fen)).unwrap();
        assert_eq!(
            pos.static_eval(),
            -mirror.static_eval(),
            "{fen} and its colour mirror were not opposite"
        );
    }
}

/// Flips a FEN vertically and swaps the piece colours, producing the colour mirror of the
/// position. Only the piece-placement field affects the evaluation, so the remaining fields are
/// set to neutral values.
fn colour_mirror_fen(fen: &str) -> String {
    let board = fen.split(' ').next().unwrap();
    let mirrored = board
        .split('/')
        .rev()
        .map(|rank| {
            rank.chars()
                .map(|c| {
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else if c.is_ascii_lowercase() {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("{mirrored} w - - 0 1")
}

/// The piece-square scores are interpolated by the game phase, so the same positional feature
/// can be worth opposite amounts in the opening and the endgame. A king in the centre is
/// exposed while the heavy pieces are on but active once they are gone, so the evaluation must
/// reward in the endgame the very central king it penalises in the middlegame. A single set of
/// untapered tables could not express both.
#[test]
fn piece_square_scores_are_tapered_by_game_phase() {
    chess::init::init_globals();

    let eval = |fen: &str| Position::from_fen(fen).unwrap().static_eval();

    // Full queens and rooks on both sides: a middlegame. With everything else symmetric, moving
    // only White's king off its home square to the centre must lower White's score.
    let king_home_mg = eval("r2qk2r/8/8/8/8/8/8/R2QK2R w - - 0 1");
    let king_centre_mg = eval("r2qk2r/8/8/8/4K3/8/8/R2Q3R w - - 0 1");
    assert!(
        king_home_mg > king_centre_mg,
        "a central king was not penalised in the middlegame ({king_home_mg} !> {king_centre_mg})"
    );

    // The same two king squares with the heavy pieces removed: an endgame. Now the central king
    // must score higher than the one still on the back rank.
    let king_home_eg = eval("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    let king_centre_eg = eval("4k3/8/8/8/4K3/8/8/8 w - - 0 1");
    assert!(
        king_centre_eg > king_home_eg,
        "a central king was not rewarded in the endgame ({king_centre_eg} !> {king_home_eg})"
    );
}

/// A table warmed at one halfmove clock must return the same score when the identical
/// position is searched at a materially different clock. Before evaluation became
/// position-intrinsic the warm result was computed under the warming clock and silently reused.
#[test]
fn warm_table_reuse_agrees_across_materially_different_halfmove_clocks() {
    chess::init::init_globals();

    let score_at = |halfmove_clock: u32, table: &Table| {
        let fen = format!("4k3/8/8/8/8/5N2/8/Q3K3 w - - {halfmove_clock} 1");
        let pos = Position::from_fen(&fen).unwrap();
        let flag = AtomicBool::new(false);
        let mut search = Search::new(pos, &flag, None, table);
        // Isolate the halfmove-clock property under test from move-ordering effects. A warm
        // table supplies hash moves that reorder the search, and late-move reduction keys off
        // that order, so with it on a warm and a cold search legitimately reduce different moves
        // and can disagree for reasons that have nothing to do with the clock. Disabling the
        // reduction (and the extensions) keeps this test measuring only what it is named for.
        search.lmr_disabled = true;
        search.extensions_disabled = true;
        search.run::<Master>(4).unwrap().score
    };

    // Warm the table at a low clock, then search the same position at a high one. A cold
    // reference search at the high clock must agree with the warm result.
    let warm_table = Table::new(16);
    let _ = score_at(0, &warm_table);
    let warm = score_at(80, &warm_table);

    let cold = score_at(80, &Table::new(16));

    assert_eq!(
        warm, cold,
        "a warm table changed the score at a different halfmove clock"
    );
}

/// Position-intrinsic evaluation is not on its own enough. A stored value never accounts for
/// the fifty-move rule, because a node whose subtree claimed the draw is not written at all.
/// Reusing such a value where the boundary *is* within reach scores a dead-drawn line as if it
/// played on, so the read side must refuse the cutoff.
///
/// The position below establishes the premise that gate exists for: one key, two materially
/// different true values, told apart only by the halfmove clock. White is a queen and a knight
/// up against a bare king, which the tapered evaluation scores at 1295 with its placement. At
/// clock 96 a four-ply search sees that every quiet continuation runs the clock to 100 and
/// draws, so its best line is to hang the queen: the king's capture resets the clock and leaves
/// White only a knight up, worth 244. Reusing the 1295 where the 244 applies is the defect.
#[test]
fn the_same_key_is_worth_different_scores_at_different_halfmove_clocks() {
    chess::init::init_globals();

    let score_at = |halfmove_clock: u32| {
        let fen = format!("4k3/8/8/8/8/5N2/8/Q3K3 w - - {halfmove_clock} 1");
        let pos = Position::from_fen(&fen).unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(pos, &flag, None, &table);
        // The exact scores below are properties of the four-ply search and the evaluation. Hold
        // the search to its nominal depth: the check-evasion extension would search this
        // heavy-check position deeper and move the numbers, without bearing on the clock gate
        // this test exists to pin. Late-move reduction and the transposition-miss depth reduction
        // are off for the same reason — both alter the effective depth without touching the clock.
        search.lmr_disabled = true;
        search.extensions_disabled = true;
        search.iir_disabled = true;
        search.run::<Master>(4).unwrap().score
    };

    assert_eq!(
        score_at(0),
        Score::cp(1295),
        "material is intact at a fresh clock"
    );
    assert_eq!(
        score_at(96),
        Score::cp(244),
        "near the boundary the queen must be given up to reset the clock"
    );
}

/// The main search must refuse a stored cutoff once the fifty-move boundary is
/// within the stored entry's reach.
///
/// Seeding the entry directly, rather than warming the table with a real search, is deliberate:
/// it pins the cutoff path under test instead of depending on which keys a warming search
/// happens to leave behind and at what depth. The previous revision's warm-table test asserted
/// only that two searches agreed, which held whether or not the gate was present.
#[test]
fn the_main_search_refuses_a_stored_cutoff_near_the_fifty_move_boundary() {
    chess::init::init_globals();

    // Bare kings: the true value is 0, so a seeded 300 can only come from the table.
    let seeded_score = Score::cp(300);
    let seeded_depth = 8;

    let score_at = |halfmove_clock: u32| {
        let fen = format!("k7/8/8/8/8/8/8/K7 w - - {halfmove_clock} 1");
        let position = Position::from_fen(&fen).unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);

        // Step 4 only takes a cutoff when the entry also carries a usable move.
        let moves = position.generate::<BasicMoveList, AllGen, Legal>();
        let mov = *moves
            .iter()
            .find(|m| format!("{m}").contains("a1b2"))
            .expect("the king move must be legal");
        table.store(
            position.zobrist().0,
            seeded_score,
            None,
            seeded_depth,
            Bound::Exact,
            &mov,
        );

        let mut search = Search::new(position, &flag, None, &table);
        search.pvt = PVTable::new(4);
        search.search::<Master, NonPv>(Score::cp(299), Score::cp(300), 4, 0)
    };

    assert_eq!(
        score_at(0),
        Some(seeded_score),
        "well below the boundary the stored cutoff must still be taken"
    );

    // 90 + 8 + 16 exceeds 100, so the rule is within the entry's reach and the value it was
    // computed under no longer applies. With the cutoff refused the position is searched, the
    // true value of 0 is far below the window, and the null-window search fails low on alpha.
    assert_eq!(
        score_at(90),
        Some(Score::cp(299)),
        "a stored value was reused where the fifty-move rule is within its reach"
    );
}

/// Quiescence probes the same table and needs the same gate.
#[test]
fn quiescence_refuses_a_stored_cutoff_near_the_fifty_move_boundary() {
    chess::init::init_globals();

    let score_at = |halfmove_clock: u32| {
        let fen = format!("k7/8/8/8/8/8/8/K7 w - - {halfmove_clock} 1");
        let position = Position::from_fen(&fen).unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        table.store(
            position.zobrist().0,
            Score::cp(300),
            None,
            8,
            Bound::Exact,
            &Move::null(),
        );

        let mut search = Search::new(position, &flag, None, &table);
        search.quiesce::<Master, NonPv>(Score::cp(299), Score::cp(300), 0)
    };

    assert_eq!(score_at(0), Some(Score::cp(300)));

    // As above, refusing the cutoff leaves a stand-pat of 0 below the window, so quiescence
    // fails low on alpha rather than returning the seeded score.
    assert_eq!(
        score_at(90),
        Some(Score::cp(299)),
        "quiescence reused a stored value across the fifty-move boundary"
    );
}

/// The clock gate must be a boundary condition, not a blanket disable: reuse has to remain
/// available at the clocks a search actually spends most of its time at.
#[test]
fn the_clock_gate_only_bites_near_the_fifty_move_boundary() {
    chess::init::init_globals();

    let permits = |halfmove_clock: u32, entry_depth: u8| {
        let fen = format!("4k3/8/8/8/8/5N2/8/Q3K3 w - - {halfmove_clock} 1");
        let pos = Position::from_fen(&fen).unwrap();
        let flag = AtomicBool::new(false);
        let tt = Table::new(1);
        let search = Search::new(pos, &flag, None, &tt);
        search.clock_permits_tt_reuse(entry_depth)
    };

    assert!(permits(0, 8), "reuse must be available at a fresh clock");
    assert!(permits(60, 8), "reuse must survive an ordinary quiet phase");

    // 83 + 8 + 16 slack reaches exactly 100, the fifty-move boundary.
    assert!(!permits(83, 8), "reuse must stop at the boundary");
    assert!(!permits(96, 4), "reuse must stop close to the boundary");

    // Deeper entries reach further, so they must be cut off sooner.
    assert!(permits(60, 8) && !permits(60, 24));
}

/// Plays a four-ply king shuffle, returning a position whose history already contains one
/// earlier occurrence of itself. A search from here can reach the threefold below the root.
fn position_repeated_once() -> Position {
    let mut pos = Position::from_fen("6k1/8/8/8/8/8/8/1K6 w - - 0 1").unwrap();

    for san in ["b1a1", "g8h8", "a1b1", "h8g8"] {
        let moves = pos.generate::<BasicMoveList, AllGen, Legal>();
        let mov = *moves
            .iter()
            .find(|m| format!("{m}").contains(san))
            .expect("shuffle move must be legal");
        pos.make_move(&mov);
    }

    pos
}

/// A value produced by a repetition claim depends on the moves played before the root,
/// which the key does not cover, so it must not reach the table at all.
#[test]
fn a_repetition_derived_value_is_not_stored_in_the_table() {
    chess::init::init_globals();

    let position = position_repeated_once();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);

    // Driven at a single fixed depth rather than through iterative deepening. Four plies are
    // needed to reach the third occurrence, so a deepening search would first store legitimate
    // history-independent results from its shallower iterations and mask the suppression.
    // The transposition-miss depth reduction is off so the cold-table root keeps its full four
    // plies: with it on, the move-less root would be reduced and the search would stop short of
    // the repetition this test needs to reach.
    search.iir_disabled = true;
    search.pvt = PVTable::new(4);
    search
        .search::<Master, Root>(Score::INF_N, Score::INF_P, 4, 0)
        .unwrap();

    assert!(
        search.history_draws > 0,
        "the test position must actually exercise a repetition claim"
    );
    assert!(
        table.probe(position.zobrist().0).is_none(),
        "a repetition-contaminated value must not be written to the table"
    );
}

/// The same holds for the fifty-move rule, the other draw the key does not cover: a
/// subtree that crosses the boundary produces a value that only applies at this clock.
#[test]
fn a_fifty_move_derived_value_is_not_stored_in_the_table() {
    chess::init::init_globals();

    // Two plies into the search the clock reaches 100 and the draw is claimed below the root.
    let position = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 98 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);

    search.pvt = PVTable::new(3);
    search
        .search::<Master, Root>(Score::INF_N, Score::INF_P, 3, 0)
        .unwrap();

    assert!(
        search.history_draws > 0,
        "the test position must actually cross the fifty-move boundary"
    );
    assert!(
        table.probe(position.zobrist().0).is_none(),
        "a clock-contaminated value must not be written to the table"
    );
}

/// A position whose subtree never claimed a history-sensitive draw is ordinary
/// position-intrinsic information and must still be stored, so the policy above is not a
/// blanket suppression of the table.
#[test]
fn a_history_independent_value_is_still_stored_in_the_table() {
    chess::init::init_globals();

    let position = Position::from_fen("4k3/8/8/8/8/5N2/8/Q3K3 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);

    let draws_before = search.history_draws;
    search.run::<Master>(3).unwrap();

    assert_eq!(
        search.history_draws, draws_before,
        "this position must not claim a history-sensitive draw"
    );
    assert!(
        !table.probe(position.zobrist().0).is_none(),
        "a history-independent value must still be stored"
    );
}

/// Both the main search and quiescence must claim the fifty-move draw at the same boundary,
/// and that boundary is 100 plies rather than 50. Quiescence once compared the clock against
/// 50, reporting a draw at 25 moves — a whole half of the legal range in which the two searches
/// disagreed about whether the game was already over.
///
/// The sweep covers every clock across that former disagreement, from the old boundary to the
/// real one, rather than sampling three points: the defect was a wrong constant, so the test
/// that pins it has to walk the range the constant governs.
#[test]
fn both_searches_claim_the_fifty_move_draw_at_the_same_hundred_ply_boundary() {
    chess::init::init_globals();

    // No captures and no checks, so quiescence stands pat unless the draw fires. The material
    // value is a queen and a pawn up, nowhere near zero, so a zero score can only be the claim.
    //
    // The pawn is what makes the main-search leg meaningful. Without it every white move is
    // quiet, so from clock 99 a one-ply search legitimately finds the draw on the next ply and
    // scores zero whether or not the root position is itself drawn. A pawn push resets the
    // clock, so below the boundary the search always has an escape and a zero score still means
    // only one thing.
    for halfmove_clock in 50..=100 {
        let fen = format!("4k3/8/8/8/8/8/P7/Q3K3 w - - {halfmove_clock} 1");
        let position = Position::from_fen(&fen).unwrap();
        let expected_draw = halfmove_clock >= 100;

        assert_eq!(
            position.fifty_move_rule_reached(),
            expected_draw,
            "the rule predicate disagreed at halfmove clock {halfmove_clock}"
        );

        let flag = AtomicBool::new(false);

        let quiescence_table = Table::new(1);
        let mut quiescence = Search::new(position.clone(), &flag, None, &quiescence_table);
        assert_eq!(
            quiescence.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0) == Some(Score::zero()),
            expected_draw,
            "quiescence disagreed at halfmove clock {halfmove_clock}"
        );

        let main_table = Table::new(1);
        let mut main = Search::new(position, &flag, None, &main_table);
        assert_eq!(
            main.run::<Master>(1).unwrap().score == Score::zero(),
            expected_draw,
            "the main search disagreed at halfmove clock {halfmove_clock}"
        );
    }
}

#[test]
fn quiescence_does_not_use_a_search_score_as_static_evaluation() {
    chess::init::init_globals();

    let position = Position::from_fen("k7/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    table.store(
        position.zobrist().0,
        Score::cp(300),
        None,
        8,
        Bound::Exact,
        &Move::null(),
    );
    let mut search = Search::new(position, &flag, None, &table);

    assert_eq!(
        search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0),
        Some(Score::zero())
    );
}

/// Seeds an entry for a bare-king position whose true value is zero, so any non-zero score the
/// search returns can only have come out of the table. Seeding directly rather than warming
/// with a real search pins the cutoff path under test instead of depending on what a warming
/// search happens to leave behind.
fn score_from_seeded_entry(
    seeded: Score,
    bound: Bound,
    mov: &Move,
    depth: u8,
) -> (NodeResult, usize) {
    let position = Position::from_fen("k7/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    table.store(position.zobrist().0, seeded, None, depth, bound, mov);

    let mut search = Search::new(position, &flag, None, &table);
    search.pvt = PVTable::new(4);
    let score = search.search::<Master, NonPv>(Score::cp(299), Score::cp(300), 4, 0);

    (score, search.trace.hash_collisions())
}

/// A checkmated or stalemated node, and every node whose moves all failed low, stores its value
/// with no move at all. Gating the main search's score reuse on the presence of a playable
/// stored move made exactly those entries — the most certain ones in the table — permanently
/// unusable. Reuse must depend on the entry being verified, not on it carrying a move.
#[test]
fn a_verified_entry_without_a_move_still_cuts_off_the_main_search() {
    chess::init::init_globals();

    let seeded = Score::cp(300);
    let (score, _) = score_from_seeded_entry(seeded, Bound::Exact, &Move::null(), 8);

    assert_eq!(
        score,
        Some(seeded),
        "a move-less entry deep enough to cut off was ignored"
    );
}

/// The same holds when the entry does carry a move but that move cannot be played here. The
/// full-key check inside `Table::probe` is what establishes identity; an unplayable move only
/// means the entry supplies no ordering hint, and is recorded as the genuine Zobrist collision
/// it must be. Both searches therefore treat the score and the move independently.
#[test]
fn an_unplayable_stored_move_costs_ordering_but_not_the_score() {
    chess::init::init_globals();

    // No piece stands on e4 in the seeded position, so this move is not playable there.
    let unplayable = Move::build(Square::E4, Square::E5, None, MoveType::QUIET);
    let seeded = Score::cp(300);
    let (score, collisions) = score_from_seeded_entry(seeded, Bound::Exact, &unplayable, 8);

    assert_eq!(
        score,
        Some(seeded),
        "an unplayable ordering move suppressed a verified score"
    );
    assert_eq!(
        collisions, 1,
        "an unplayable move on a verified entry must be counted as a collision"
    );
}

/// Quiescence must publish its results, not merely consume other nodes'. A quiet position has
/// nothing to search, so the value it stores is its stand pat, recorded at the reserved
/// quiescence draft.
#[test]
fn quiescence_publishes_its_result_at_the_reserved_draft() {
    chess::init::init_globals();

    let position = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position.clone(), &flag, None, &table);

    let score = search
        .quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0)
        .unwrap();

    let entry = table
        .probe(position.zobrist().0)
        .expect("quiescence must store a completed result");
    assert_eq!(entry.score(), score);
    assert_eq!(entry.depth(), Search::QUIESCENCE_DRAFT);
    assert_eq!(entry.bound(), Bound::Exact);
}

/// The reserved draft is what keeps the two searches' entries apart. A capture-only value can
/// never satisfy a main-search node's depth requirement, so seeding one cannot change a
/// depth-one search: the result must match a search that never saw the entry at all.
#[test]
fn a_quiescence_entry_cannot_satisfy_a_main_search_depth_requirement() {
    chess::init::init_globals();

    let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);

    let score_with = |table: &Table| {
        let mut search = Search::new(position.clone(), &flag, None, table);
        search.pvt = PVTable::new(1);
        search.search::<Master, NonPv>(Score::cp(299), Score::cp(300), 1, 0)
    };

    let seeded = Table::new(1);
    seeded.store(
        position.zobrist().0,
        Score::cp(300),
        None,
        Search::QUIESCENCE_DRAFT,
        Bound::Exact,
        &Move::null(),
    );

    assert_eq!(
        score_with(&seeded),
        score_with(&Table::new(1)),
        "a quiescence-draft entry was reused by a depth-one main search"
    );
}

/// A quiescence subtree that a stop cut short has examined only some of its captures, so its
/// alpha is not a bound on anything. It must not reach the table, on the same terms as the
/// main search's aborted subtrees.
#[test]
fn an_aborted_quiescence_subtree_publishes_nothing() {
    chess::init::init_globals();

    // A capture chain, so quiescence recurses rather than standing pat immediately, and the
    // abort lands part way through the tree.
    let position = Position::from_fen("4k3/8/8/3q4/4P3/5N2/8/4K3 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);
    search.root_fallback_ready = true;
    search.abort_after_nodes = Some(1);

    assert_eq!(
        search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0),
        None,
        "the abort must actually cut the subtree short"
    );
    assert!(
        table.probe(position.zobrist().0).is_none(),
        "an aborted quiescence subtree published an entry"
    );
}

/// Quiescence follows quiet check evasions, which advance the halfmove clock, so it can claim a
/// fifty-move draw below its own root. That value depends on the moves played before the
/// search, which the key does not cover, and is suppressed exactly as the main search
/// suppresses its own.
#[test]
fn a_history_sensitive_quiescence_value_is_not_stored() {
    chess::init::init_globals();

    // White is in check from the rook, so quiescence follows the evasions rather than standing
    // pat. Every evasion is a quiet king move, which advances the clock from 99 to the boundary
    // and makes the child claim the draw — below this node's own root, which is not yet drawn.
    let position = Position::from_fen("k3r3/8/8/8/8/8/8/4K3 w - - 99 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);

    search
        .quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0)
        .unwrap();

    assert!(
        search.history_draws > 0,
        "the test position must actually cross the fifty-move boundary below the root"
    );
    assert!(
        table.probe(position.zobrist().0).is_none(),
        "a clock-contaminated quiescence value was published"
    );
}

/// The point of the table is that a repeated search costs less and answers the same. Re-running
/// each position against the table its own first search filled must not change the score or the
/// move, and must not cost more nodes than the cold search did.
#[test]
fn a_warm_table_matches_the_cold_result_and_never_costs_more_nodes() {
    chess::init::init_globals();

    let positions = [
        // A forced mate: terminal values, stored without a move, and the entries a move-gated
        // cutoff could never reuse.
        ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6),
        ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 6),
        // Tactical material wins, where the saving comes from ordinary bound reuse.
        ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 5),
        ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6),
    ];

    for (fen, depth) in positions {
        let position = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);

        let mut cold = Search::new(position.clone(), &flag, None, &table);
        let cold_result = cold.run::<Master>(depth).unwrap();
        let cold_nodes = cold.trace.all_nodes_visited();

        let mut warm = Search::new(position, &flag, None, &table);
        let warm_result = warm.run::<Master>(depth).unwrap();
        let warm_nodes = warm.trace.all_nodes_visited();

        assert_eq!(
            warm_result.score, cold_result.score,
            "{fen}: a warm table changed the score"
        );
        assert_eq!(
            warm_result.best_move, cold_result.best_move,
            "{fen}: a warm table changed the best move"
        );
        assert!(
            warm_nodes <= cold_nodes,
            "{fen}: the warm search cost more nodes ({warm_nodes}) than the cold one \
             ({cold_nodes})"
        );
    }
}

/// The futility margin grows monotonically with remaining depth: a subtree with more room to
/// realise a latent gain is granted a more generous allowance, so pruning stays more cautious
/// further from the horizon.
#[test]
fn futility_margin_grows_with_depth() {
    for depth in 1..FUTILITY_MAX_DEPTH {
        assert!(
            futility_margin(depth) < futility_margin(depth + 1),
            "futility margin did not grow from depth {depth} to {}",
            depth + 1
        );
    }
}

/// The forward-pruning guards must not change the result of a search on a sound position: with
/// futility and null-move pruning switched off, a fixed-depth search of each position returns
/// exactly the score and best move it returns with them on. This is the guard-correctness
/// contract — where the pruning fires it only skips work the full search would have discarded
/// anyway, so these known-answer searches are identical either way — and it also confirms the
/// test toggle actually reaches both steps.
#[test]
fn forward_pruning_does_not_change_sound_search_results() {
    chess::init::init_globals();

    // Forced mates and clean material wins across a range of fixed depths. Each has an
    // unambiguous result that pruning must not disturb.
    let positions = [
        ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6),
        ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 8),
        ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 5),
        ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6),
        (
            "2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14",
            5,
        ),
    ];

    for (fen, depth) in positions {
        let position = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);

        let pruned_table = Table::new(16);
        let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
        let pruned_result = pruned.run::<Master>(depth).unwrap();

        let plain_table = Table::new(16);
        let mut plain = Search::new(position, &flag, None, &plain_table);
        plain.forward_pruning_disabled = true;
        let plain_result = plain.run::<Master>(depth).unwrap();

        assert_eq!(
            pruned_result.score, plain_result.score,
            "{fen}: forward pruning changed the score"
        );
        assert_eq!(
            pruned_result.best_move, plain_result.best_move,
            "{fen}: forward pruning changed the best move"
        );
    }
}

/// Forward pruning must earn its keep: on a quiet middlegame position with a decisive advantage
/// — where the null-move and futility guards are satisfied deep in the tree — the pruned search
/// visits strictly fewer nodes than the same search with pruning switched off, at the same
/// fixed depth. This confirms the pruning actually fires rather than being dead code behind its
/// guards.
#[test]
fn forward_pruning_reduces_the_search_tree() {
    chess::init::init_globals();

    // A quiet position a few pawns up for White, with pieces on the board so the zugzwang guard
    // is satisfied and null-move pruning can fire.
    let position =
        Position::from_fen("r3k2r/pppq1ppp/2np1n2/4p3/2B1P3/2NP1N2/PPP2PPP/R2Q1RK1 w kq - 0 1")
            .unwrap();
    let flag = AtomicBool::new(false);
    let depth = 6;

    let pruned_table = Table::new(16);
    let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
    pruned.run::<Master>(depth).unwrap();
    let pruned_nodes = pruned.trace.all_nodes_visited();

    let plain_table = Table::new(16);
    let mut plain = Search::new(position, &flag, None, &plain_table);
    plain.forward_pruning_disabled = true;
    plain.run::<Master>(depth).unwrap();
    let plain_nodes = plain.trace.all_nodes_visited();

    assert!(
        pruned_nodes < plain_nodes,
        "forward pruning did not reduce the tree: {pruned_nodes} pruned vs {plain_nodes} plain"
    );
}

/// The reverse futility margin must widen with remaining depth and stay a centipawn quantity, so
/// a deeper node demands a larger surplus above beta before it is pruned and the comparison never
/// straddles the centipawn/mate boundary the pruning guard relies on.
#[test]
fn reverse_futility_margin_grows_with_depth() {
    let mut previous = reverse_futility_margin(1);
    assert!(previous.is_cp());
    for depth in 2..=REVERSE_FUTILITY_MAX_DEPTH {
        let margin = reverse_futility_margin(depth);
        assert!(
            margin.is_cp(),
            "depth {depth}: margin left the centipawn band"
        );
        assert!(
            margin > previous,
            "depth {depth}: margin {margin:?} did not exceed shallower {previous:?}"
        );
        previous = margin;
    }
}

/// Reverse futility pruning must not change the result of a search on a sound position: with the
/// whole-node prune switched off, a fixed-depth search of each position returns exactly the score
/// and best move it returns with the prune on. These are forced mates and decisive material wins,
/// where the guards — non-PV, not in check, shallow draft, non-mate beta bound — keep the prune to
/// nodes the full search would have failed high on anyway, so the known answer is identical either
/// way and the toggle is confirmed to reach the step.
#[test]
fn reverse_futility_pruning_does_not_change_sound_search_results() {
    chess::init::init_globals();

    let positions = [
        ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6),
        ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 8),
        ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 5),
        ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6),
        (
            "2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14",
            5,
        ),
    ];

    for (fen, depth) in positions {
        let position = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);

        let pruned_table = Table::new(16);
        let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
        let pruned_result = pruned.run::<Master>(depth).unwrap();

        let plain_table = Table::new(16);
        let mut plain = Search::new(position, &flag, None, &plain_table);
        plain.rfp_disabled = true;
        let plain_result = plain.run::<Master>(depth).unwrap();

        assert_eq!(
            pruned_result.score, plain_result.score,
            "{fen}: reverse futility pruning changed the score"
        );
        assert_eq!(
            pruned_result.best_move, plain_result.best_move,
            "{fen}: reverse futility pruning changed the best move"
        );
    }
}

/// Reverse futility pruning must earn its keep: on a quiet position where one side stands clearly
/// ahead — so its shallow non-PV nodes clear beta by the margin — the pruned search visits
/// strictly fewer nodes than the same fixed-depth search with the prune switched off. This
/// confirms the step actually fires rather than being dead code behind its guards.
#[test]
fn reverse_futility_pruning_reduces_the_search_tree() {
    chess::init::init_globals();

    // A quiet position a clean piece up for White, with the material edge the static evaluation
    // can see and pieces still on the board so shallow nodes recur throughout the tree.
    let position =
        Position::from_fen("r3k2r/pppq1ppp/2np1n2/4p3/2B1P3/2NP1N2/PPP2PPP/R2Q1RK1 w kq - 0 1")
            .unwrap();
    let flag = AtomicBool::new(false);
    let depth = 6;

    let pruned_table = Table::new(16);
    let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
    pruned.run::<Master>(depth).unwrap();
    let pruned_nodes = pruned.trace.all_nodes_visited();

    let plain_table = Table::new(16);
    let mut plain = Search::new(position, &flag, None, &plain_table);
    plain.rfp_disabled = true;
    plain.run::<Master>(depth).unwrap();
    let plain_nodes = plain.trace.all_nodes_visited();

    assert!(
        pruned_nodes < plain_nodes,
        "reverse futility pruning did not reduce the tree: {pruned_nodes} pruned vs {plain_nodes} plain"
    );
}

/// Late-move reduction must not change the result of a search on a position whose best line is
/// within reach of the reduced scout: the full-depth re-search restores any move the reduction
/// underestimated, so a reduced search returns the same score and the same best move as one with
/// the reduction switched off. These are clean, decisive middlegame and endgame positions where
/// no deep tactic hides beyond the reduced horizon — exactly where the reduction is meant to be
/// transparent — so any divergence would signal a broken re-search rather than an accepted
/// heuristic loss. (Positions with a deep forced mate are deliberately excluded: there the
/// reduction can defer the mate score by an iteration, which is expected and covered elsewhere.)
#[test]
fn late_move_reduction_does_not_change_sound_search_results() {
    chess::init::init_globals();

    let positions = [
        ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 5),
        ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 5),
        (
            "2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14",
            6,
        ),
    ];

    for (fen, depth) in positions {
        let position = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);

        let reduced_table = Table::new(16);
        let mut reduced = Search::new(position.clone(), &flag, None, &reduced_table);
        let reduced_result = reduced.run::<Master>(depth).unwrap();

        let full_table = Table::new(16);
        let mut full = Search::new(position, &flag, None, &full_table);
        full.lmr_disabled = true;
        let full_result = full.run::<Master>(depth).unwrap();

        assert_eq!(
            reduced_result.score, full_result.score,
            "{fen}: late-move reduction changed the score"
        );
        assert_eq!(
            reduced_result.best_move, full_result.best_move,
            "{fen}: late-move reduction changed the best move"
        );
    }
}

/// Late-move reduction must earn its keep: on a quiet middlegame position with pieces enough for
/// the reduction to fire deep in the tree, the reduced search visits strictly fewer nodes than
/// the same fixed-depth search with the reduction switched off. This confirms the reduction is
/// actually cutting work rather than being neutralised by its own re-searches.
#[test]
fn late_move_reduction_reduces_the_search_tree() {
    chess::init::init_globals();

    let position =
        Position::from_fen("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14").unwrap();
    let flag = AtomicBool::new(false);
    let depth = 6;

    let reduced_table = Table::new(16);
    let mut reduced = Search::new(position.clone(), &flag, None, &reduced_table);
    reduced.run::<Master>(depth).unwrap();
    let reduced_nodes = reduced.trace.all_nodes_visited();

    let full_table = Table::new(16);
    let mut full = Search::new(position, &flag, None, &full_table);
    full.lmr_disabled = true;
    full.run::<Master>(depth).unwrap();
    let full_nodes = full.trace.all_nodes_visited();

    assert!(
        reduced_nodes < full_nodes,
        "late-move reduction did not reduce the tree: {reduced_nodes} reduced vs {full_nodes} full"
    );
}

/// A quiet, piece-heavy middlegame position whose fixed-depth tree recurses widely, so a
/// depth-reducing heuristic has room to visibly cut work. Shared by the internal-iterative-reduction
/// tests below.
const IIR_TEST_FEN: &str = "r3k2r/pppq1ppp/2np1n2/4p3/2B1P3/2NP1N2/PPP2PPP/R2Q1RK1 w kq - 0 1";

/// Internal iterative reduction must earn its keep in PV nodes: a PV node with no transposition-table
/// move to search first is trimmed by [`IIR_PV_REDUCTION`] plies, so a search visits strictly fewer
/// nodes with the reduction on than off. The depth is held below [`IIR_NON_PV_MIN_DEPTH`] so no node
/// in the tree qualifies for the non-PV cut at Step 13 — the reduction of the tree is therefore
/// attributable to Step 11 alone. The search is driven directly at one fixed depth rather than
/// through iterative deepening so the table starts empty and every node's first visit is a genuine
/// miss, which is exactly the condition the reduction keys on.
#[test]
fn internal_iterative_reduction_reduces_a_pv_search_tree() {
    chess::init::init_globals();

    let position = Position::from_fen(IIR_TEST_FEN).unwrap();
    let flag = AtomicBool::new(false);
    let depth = 6;

    let nodes = |iir_disabled: bool| {
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);
        search.pvt = PVTable::new(depth as u8);
        search.iir_disabled = iir_disabled;
        search
            .search::<Master, Pv>(Score::INF_N, Score::INF_P, depth, 0)
            .unwrap();
        search.trace.all_nodes_visited()
    };

    let reduced = nodes(false);
    let full = nodes(true);
    assert!(
        reduced < full,
        "internal iterative reduction did not reduce the PV tree: {reduced} reduced vs {full} full"
    );
}

/// Internal iterative reduction must earn its keep in non-PV nodes too: at or above
/// [`IIR_NON_PV_MIN_DEPTH`] a non-PV node with no transposition-table move is trimmed by
/// [`IIR_NON_PV_REDUCTION`] plies. A pure non-PV search never reaches Step 11's PV cut, so any
/// difference here is Step 13's alone. The other depth-shrinking heuristics are switched off so the
/// tree is wide enough for the two-ply cut to register unmistakably rather than being swallowed by
/// forward pruning; only the reduction under test varies between the two runs. The window sits above
/// the position's true value so the node fails low and must actually search its moves.
#[test]
fn internal_iterative_reduction_reduces_a_non_pv_search_tree() {
    chess::init::init_globals();

    let position = Position::from_fen(IIR_TEST_FEN).unwrap();
    let flag = AtomicBool::new(false);
    let depth = IIR_NON_PV_MIN_DEPTH;

    let nodes = |iir_disabled: bool| {
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);
        search.pvt = PVTable::new(depth as u8);
        search.iir_disabled = iir_disabled;
        search.forward_pruning_disabled = true;
        search.rfp_disabled = true;
        search.lmr_disabled = true;
        search.lmp_disabled = true;
        search
            .search::<Master, NonPv>(Score::cp(200), Score::cp(201), depth, 0)
            .unwrap();
        search.trace.all_nodes_visited()
    };

    let reduced = nodes(false);
    let full = nodes(true);
    assert!(
        reduced < full,
        "internal iterative reduction did not reduce the non-PV tree: \
         {reduced} reduced vs {full} full"
    );
}

/// The reduction must fire on a genuine absence of a table move and never on a Zobrist collision. A
/// full-key hit whose stored move cannot be played here belongs to a foreign position and says
/// nothing about whether this node has been explored, so it must not stand in for a real miss.
///
/// The two runs are made identical except for one seeded entry. At depth two only the root can
/// change: Step 11's cut is floored at one ply, so a child at depth one is left untouched, and the
/// grandchildren fall straight into quiescence — the root is the sole node whose reduction is
/// observable. With an empty table the root is a genuine miss and the reduction fires, cutting the
/// tree; with the root's slot seeded to a full-key entry carrying an unplayable move, the collision
/// guard must suppress the reduction, leaving the tree identical to a search that never reduces.
#[test]
fn internal_iterative_reduction_ignores_a_transposition_collision() {
    chess::init::init_globals();

    let position = Position::from_fen(IIR_TEST_FEN).unwrap();
    let flag = AtomicBool::new(false);
    let depth = 2;

    // No piece stands on a4 in this position, so this move cannot be played here: a full-key hit
    // carrying it is a genuine collision, not an ordering hint.
    let unplayable = Move::build(Square::A4, Square::A5, None, MoveType::QUIET);

    let nodes = |seed_collision: bool, iir_disabled: bool| {
        let table = Table::new(16);
        if seed_collision {
            table.store(
                position.zobrist().0,
                Score::cp(0),
                None,
                8,
                Bound::Exact,
                &unplayable,
            );
        }
        let mut search = Search::new(position.clone(), &flag, None, &table);
        search.pvt = PVTable::new(depth as u8);
        search.iir_disabled = iir_disabled;
        search
            .search::<Master, Pv>(Score::INF_N, Score::INF_P, depth, 0)
            .unwrap();
        search.trace.all_nodes_visited()
    };

    let miss_reduced = nodes(false, false);
    let miss_full = nodes(false, true);
    let collision_reduced = nodes(true, false);

    // A genuine miss reduces the tree.
    assert!(
        miss_reduced < miss_full,
        "the reduction did not fire on a genuine miss: {miss_reduced} vs {miss_full}"
    );
    // A collision does not: the reduction-enabled search matches the un-reduced tree exactly,
    // proving the collision guard took the no-reduce path rather than treating it as a miss.
    assert_eq!(
        collision_reduced, miss_full,
        "the reduction fired on a collision instead of treating it as a hit: \
         {collision_reduced} vs {miss_full}"
    );
}

/// Late-move (move-count) pruning must earn its keep: on a quiet middlegame position with a long
/// tail of quiet moves at every node, the pruned search visits strictly fewer nodes than the same
/// fixed-depth search with the prune switched off. This confirms the technique actually discards
/// work rather than sitting dead behind its guards.
#[test]
fn late_move_pruning_reduces_the_search_tree() {
    chess::init::init_globals();

    let position =
        Position::from_fen("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14").unwrap();
    let flag = AtomicBool::new(false);
    let depth = 7;

    let pruned_table = Table::new(16);
    let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
    pruned.run::<Master>(depth).unwrap();
    let pruned_nodes = pruned.trace.all_nodes_visited();

    let plain_table = Table::new(16);
    let mut plain = Search::new(position, &flag, None, &plain_table);
    plain.lmp_disabled = true;
    plain.run::<Master>(depth).unwrap();
    let plain_nodes = plain.trace.all_nodes_visited();

    assert!(
        pruned_nodes < plain_nodes,
        "late-move pruning did not reduce the tree: {pruned_nodes} pruned vs {plain_nodes} plain"
    );
}

/// Late-move pruning discards only the tail of the quiet moves; a decisive move that wins material
/// is a winning capture, ordered far ahead of the quiet phase, so the prune must never reach it.
/// From a position whose best move is a free capture of the enemy queen, the pruned search still
/// finds that capture with a winning score, exactly as the search with the prune switched off
/// does. (The two searches' exact scores need not coincide — pruning the quiet tail deeper in the
/// tree can shift the backed-up value by a hair — so the decisive move and a winning margin are
/// what is asserted, not a bit-identical score.) This guards against the prune reaching too far up
/// the ordering and dropping a move that actually decides the position.
#[test]
fn late_move_pruning_keeps_a_decisive_capture() {
    chess::init::init_globals();

    // White to move: the rook on d1 wins the undefended black queen on d4 outright.
    let position = Position::from_fen("4k3/8/8/8/3q4/8/8/3RK3 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let depth = 5;

    let pruned_table = Table::new(16);
    let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
    let pruned_result = pruned.run::<Master>(depth).unwrap();

    let plain_table = Table::new(16);
    let mut plain = Search::new(position, &flag, None, &plain_table);
    plain.lmp_disabled = true;
    let plain_result = plain.run::<Master>(depth).unwrap();

    for (label, result) in [("pruned", &pruned_result), ("plain", &plain_result)] {
        let best = result.best_move.expect("a legal move exists");
        assert_eq!(
            (best.orig(), best.dest()),
            (Square::D1, Square::D4),
            "{label} search did not play the decisive capture"
        );
        assert!(
            result.score >= Score::cp(500),
            "{label} search lost the winning score: {:?}",
            result.score
        );
    }
}

/// The main search does not consider underpromotions: they are the final ordering phase and are
/// dropped from every non-quiescence node, on the reasoning that a rook, knight or bishop
/// promotion decides a game so rarely that resolving it can be left to quiescence. This test pins
/// that decision to observable behaviour. In the position below a knight underpromotion promotes
/// with check and forks the black king and queen, winning the queen outright — objectively far
/// stronger than the queen promotion — yet the search must decline it and promote to a queen,
/// because the knight promotion is never generated at a search node. A search that did consider
/// underpromotions here would return the knight fork; this one must not.
#[test]
fn the_main_search_does_not_select_an_underpromotion() {
    chess::init::init_globals();

    // White to move: e7-e8=N+ forks the king on c7 and the queen on g7. Every white king and rook
    // placement here keeps the promotion legal and the king off the black queen's lines.
    let position = Position::from_fen("8/2k1P1q1/8/8/8/7K/8/R7 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let depth = 5;

    let table = Table::new(16);
    let mut search = Search::new(position, &flag, None, &table);
    let result = search.run::<Master>(depth).unwrap();
    let best = result.best_move.expect("a legal move exists");

    assert_eq!(
        (best.orig(), best.dest()),
        (Square::E7, Square::E8),
        "the search abandoned the promotion entirely"
    );
    assert_eq!(
        best.promo_piece_type(),
        Some(PieceType::Queen),
        "the main search selected an underpromotion, which it must never generate"
    );
}

/// The check-evasion extension must actually deepen the subtree it fires on. From a position
/// where the side to move is in check, every root move is an evasion and each is extended by a
/// ply, so a fixed-depth search visits strictly more nodes than the same search with the
/// extension switched off. Reduction is held off on both sides so the only difference measured
/// is the extension's extra ply.
#[test]
fn the_check_evasion_extension_deepens_an_in_check_search() {
    chess::init::init_globals();

    let position = Position::from_fen("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1").unwrap();
    assert!(
        position.in_check(),
        "the test position must have the side to move in check"
    );
    let flag = AtomicBool::new(false);
    let depth = 5;

    let extended_table = Table::new(16);
    let mut extended = Search::new(position.clone(), &flag, None, &extended_table);
    extended.lmr_disabled = true;
    extended.run::<Master>(depth).unwrap();
    let extended_nodes = extended.trace.all_nodes_visited();

    let plain_table = Table::new(16);
    let mut plain = Search::new(position, &flag, None, &plain_table);
    plain.lmr_disabled = true;
    plain.extensions_disabled = true;
    plain.run::<Master>(depth).unwrap();
    let plain_nodes = plain.trace.all_nodes_visited();

    assert!(
        extended_nodes > plain_nodes,
        "the check-evasion extension did not deepen the search: \
         {extended_nodes} extended vs {plain_nodes} plain"
    );
}

#[test]
fn quiescence_ignores_tt_slot_clashes() {
    chess::init::init_globals();

    let position = Position::from_fen("k7/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let clashing_position = Position::from_fen("k7/8/8/8/8/8/8/K7 b - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    // The smallest table is a single cluster, so both positions necessarily share it.
    let table = Table::new(0);
    assert_eq!(
        table.cluster_index(position.zobrist().0),
        table.cluster_index(clashing_position.zobrist().0)
    );
    table.store(
        clashing_position.zobrist().0,
        Score::cp(300),
        None,
        8,
        Bound::Exact,
        &Move::null(),
    );
    assert!(
        table.probe(position.zobrist().0).is_none(),
        "another position's entry in the same cluster must not verify"
    );
    let mut search = Search::new(position, &flag, None, &table);

    assert_eq!(
        search.quiesce::<Master, NonPv>(Score::cp(-1), Score::zero(), 0),
        Some(Score::zero())
    );
}

/// A regression test to ensure that our search routine produces the expected results for a
/// range of positions.
#[test]
fn gives_correct_answers() {
    chess::init::init_globals();

    let suite = suite();

    for (fen, depth, lo, hi, best_moves) in suite {
        let pos = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);
        let tt = Table::new(16);
        let mut search = Search::new(pos, &flag, None, &tt);
        let result = search.run::<Master>(depth).unwrap();

        assert!(lo <= result.score, "{fen}: {} < {lo}", result.score);
        assert!(result.score <= hi, "{fen}: {} > {hi}", result.score);
        let played = result.best_move.unwrap().to_uci_string();
        assert!(
            best_moves.contains(&played.as_str()),
            "{fen}: played {played}, expected one of {best_moves:?}"
        );
    }
}

#[test]
fn typed_api_returns_completed_search() {
    chess::init::init_globals();

    let engine = SearchEngine::new(1);
    let search = engine.start(Position::start_pos(), SearchLimit::Depth(2));
    let outcome = search.wait();

    assert!(!outcome.was_cancelled());
    assert_eq!(outcome.result().unwrap().depth, 2);
    assert!(outcome.result().unwrap().best_move.is_some());
}

#[test]
fn searches_reuse_the_shared_table_until_the_owner_clears_it() {
    chess::init::init_globals();

    let mut engine = SearchEngine::new(1);
    let marker = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    assert_ne!(
        engine.table.cluster_index(marker.zobrist().0),
        engine
            .table
            .cluster_index(Position::start_pos().zobrist().0)
    );
    engine.table.store(
        marker.zobrist().0,
        Score::cp(17),
        None,
        1,
        Bound::Exact,
        &Move::null(),
    );

    engine
        .start(Position::start_pos(), SearchLimit::Depth(1))
        .wait();
    engine
        .start(Position::start_pos(), SearchLimit::Depth(1))
        .wait();
    assert!(engine.table.probe(marker.zobrist().0).is_some());

    // `clear_hash` needs an exclusive reference to the table, which is only obtainable once
    // every search has finished — the boundary that keeps a clear from racing a live worker.
    engine.clear_hash();
    assert!(engine.table.probe(marker.zobrist().0).is_none());
}

/// Dropping a handle rather than waiting on it must still leave the table unshared, so that a
/// subsequent new-game clear can take its exclusive reference. If `Drop` merely cancelled and
/// detached, the worker would outlive the handle still holding a clone of the table, and the
/// clear below would panic whenever it won the race.
#[test]
fn dropping_a_search_handle_releases_the_table_for_a_later_clear() {
    chess::init::init_globals();

    let mut engine = SearchEngine::new(1);

    // An unbounded search, so it is certainly still running at the point the handle is
    // dropped. Nothing observes its outcome: the drop is the whole subject of the test.
    drop(engine.start(Position::start_pos(), SearchLimit::Infinite));

    engine.clear_hash();
}

#[test]
fn concurrent_searches_do_not_invalidate_the_shared_generation() {
    chess::init::init_globals();

    let engine = SearchEngine::new(1);
    let marker = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
    engine.table.store(
        marker.zobrist().0,
        Score::cp(17),
        None,
        1,
        Bound::Exact,
        &Move::null(),
    );

    let first = engine.start(Position::start_pos(), SearchLimit::Depth(2));
    let second = engine.start(Position::start_pos(), SearchLimit::Depth(2));
    first.wait();
    second.wait();

    assert!(engine.table.probe(marker.zobrist().0).is_some());
}

#[test]
fn typed_api_delivers_iterative_deepening_events() {
    chess::init::init_globals();

    let engine = SearchEngine::new(1);
    let search = engine.start(Position::start_pos(), SearchLimit::Depth(2));
    let events = search.events().clone();
    let outcome = search.wait();
    let progress = events
        .try_iter()
        .filter_map(|event| match event {
            SearchEvent::Progress(progress) => Some(progress),
            SearchEvent::CurrentMove(_) => None,
        })
        .collect::<Vec<_>>();

    assert!(matches!(outcome, SearchOutcome::Completed(_)));
    assert_eq!(progress.len(), 2);
    assert_eq!(progress[0].depth, 1);
    assert_eq!(progress[1].depth, 2);
    assert!(progress.iter().all(|event| event.nodes > 0));
    assert!(progress
        .iter()
        .all(|event| !event.principal_variation.is_empty()));
}

/// FastChess reached this WAC-derived position after a long forcing line. The old search passed
/// position-relative mate bounds to child nodes by negating them without first removing one
/// ply. A cutoff value then leaked back as `Score::mate(34)`: positive with an impossible even
/// ply count, and formatting the progress event tripped Score's parity assertion on the UCI
/// driver thread.
///
/// The mate surfaces at depth seven: the previous iteration returns a non-mate centipawn score,
/// so an aspiration window centred on it first fails high on the mate and only the widening
/// re-search recovers it. This exercises that a mate reported out of a re-search still carries
/// correct distance parity. (The exact iteration the mate first appears at depends on the
/// reduction schedule and is not itself the subject; only the parity plumbing is.)
#[test]
fn child_mate_windows_preserve_distance_parity() {
    chess::init::init_globals();

    let position = Position::from_fen("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61").unwrap();
    // Which iteration the mate first appears at follows from the leaf values, so the evaluator
    // is pinned to the hand-crafted one this position was chosen against rather than left to
    // whatever network the build embeds.
    let mut engine = SearchEngine::new(1);
    engine.set_network(None);
    let search = engine.start(position, SearchLimit::Depth(7));
    let events = search.events().clone();
    let outcome = search.wait();
    let progress = events
        .try_iter()
        .filter_map(|event| match event {
            SearchEvent::Progress(progress) if progress.depth == 7 => Some(progress),
            _ => None,
        })
        .next()
        .expect("depth-seven progress must be emitted");

    assert!(matches!(outcome, SearchOutcome::Completed(Some(_))));
    assert_eq!(progress.score, Score::mate(7));
    assert!(
        crate::info::format_search_event(&SearchEvent::Progress(progress)).contains("score mate 4")
    );
}

/// A centipawn centre widens symmetrically and stays a strictly ordered, in-band window edge,
/// while any input that a centipawn offset cannot move opens the corresponding infinity. The
/// mate and max-delta cases are what keep an aspiration re-search from constructing a window a
/// mate score can never satisfy, which would loop forever.
#[test]
fn aspiration_bound_widens_clamps_and_opens_on_mate() {
    // Ordinary centipawn centre: the offset is applied verbatim on both sides.
    assert_eq!(aspiration_bound(Score::cp(30), -25), Score::cp(5));
    assert_eq!(aspiration_bound(Score::cp(30), 25), Score::cp(55));

    // Near the edge of the centipawn range the bound saturates rather than escaping it.
    assert_eq!(aspiration_bound(Score::cp(9_990), 25), Score::cp(10_000));
    assert_eq!(aspiration_bound(Score::cp(-9_990), -25), Score::cp(-10_000));

    // A half-width past the cap, and any mate centre, open straight to the matching infinity:
    // centipawns cannot bracket a mate, so the window is thrown fully open on that side.
    assert_eq!(
        aspiration_bound(Score::cp(0), ASPIRATION_MAX_DELTA + 1),
        Score::INF_P
    );
    assert_eq!(
        aspiration_bound(Score::cp(0), -(ASPIRATION_MAX_DELTA + 1)),
        Score::INF_N
    );
    assert_eq!(aspiration_bound(Score::mate(3), 25), Score::INF_P);
    assert_eq!(aspiration_bound(Score::mate(3), -25), Score::INF_N);
    assert_eq!(aspiration_bound(Score::mate(-3), 25), Score::INF_P);
}

/// A forced mate at the root, searched deep enough that aspiration is active, must still report
/// the mate. The window is centred on the previous iteration's non-mate score, so the mate
/// first fails the window high and only the widening re-search recovers it; the reported score
/// must be the true mate and must stay inside the band a node can hold.
#[test]
fn aspiration_recovers_a_forced_mate_at_the_root() {
    chess::init::init_globals();

    // Mate in five (`Score::mate(5)`, rendered as `mate 3`); the mating side is to move.
    let position = Position::from_fen("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position, &flag, None, &table);

    // Depth six is comfortably above `ASPIRATION_MIN_DEPTH`, so several iterations run under a
    // narrow window before the mate is found.
    let result = search.run::<Master>(6).unwrap();

    assert_eq!(result.score, Score::mate(5));
    assert!(result.score.is_node_score());
    assert!(result.best_move.is_some());
}

/// Once an iteration has found the mate, the next iteration centres its window on a mate score.
/// A centipawn window cannot bracket a mate, so aspiration must fall back to the full window
/// rather than build a degenerate one; the deeper iteration must still report the mate.
#[test]
fn aspiration_from_a_mate_previous_score_uses_the_full_window() {
    chess::init::init_globals();

    let position = Position::from_fen("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position, &flag, None, &table);

    // Depth eight is two plies past where the mate first appears, so at least one iteration
    // runs `aspiration_search` with a mate as its previous score.
    let result = search.run::<Master>(8).unwrap();

    assert_eq!(result.score, Score::mate(5));
    assert!(result.score.is_node_score());
}

/// Sweep the 300-position Win At Chess tactical suite and format every root score, at the
/// depths where mate scores start appearing in quantity. This is the broad counterpart to the
/// targeted window tests: it is not looking for a specific value but for any score the search
/// can reach whose rendering panics, which is how a `Display` parity violation once surfaced.
/// Debug assertions must be live for it to mean anything, so run it on a debug build:
///
/// ```text
/// cargo test -p engine -- --ignored wac_root_scores_format_without_panicking
/// ```
#[test]
#[ignore = "sweeps 900 searches; run explicitly when changing Score or the search window"]
fn wac_root_scores_format_without_panicking() {
    chess::init::init_globals();

    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../suites/wac.epd"))
        .expect("wac.epd must be readable");

    // EPD records carry only the four placement fields, so the clocks are appended.
    let positions: Vec<(String, String)> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let id = line
                .split("id \"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap_or("unknown")
                .to_string();
            let fen = format!(
                "{} {} {} {} 0 1",
                fields[0], fields[1], fields[2], fields[3]
            );
            (id, fen)
        })
        .collect();

    assert_eq!(positions.len(), 300, "the full WAC suite must be swept");

    let mut formatted = 0;
    for (id, fen) in &positions {
        for depth in [4, 5, 6] {
            let position = Position::from_fen(fen).unwrap();
            let engine = SearchEngine::new(1);
            let search = engine.start(position, SearchLimit::Depth(depth));
            let events = search.events().clone();
            let outcome = search.wait();

            assert!(
                matches!(outcome, SearchOutcome::Completed(_)),
                "{id} depth {depth} did not complete",
            );

            for event in events.try_iter() {
                if let SearchEvent::Progress(progress) = &event {
                    assert!(
                        progress.score.is_node_score(),
                        "{id} depth {depth} reported {:?}, outside the node score band",
                        progress.score,
                    );
                    // `Display` carries the parity assertions; formatting is the check.
                    let line = crate::info::format_search_event(&event);
                    assert!(line.contains("score "), "{id} depth {depth}: {line}");
                    formatted += 1;
                }
            }
        }
    }

    assert!(
        formatted >= positions.len() * 3,
        "expected at least one root score per search, got {formatted}",
    );
}

/// The window `(Score(20_100), Score(20_101))` is not contrived: it is exactly what a child
/// receives when its parent searches the null window at the very bottom of the mate band,
/// since `child_bound` is exact and both ends of that window sit above the top of the band.
/// Every score is below such an alpha. The entry clamp keeps the threshold inside the node
/// band. A collapsed window returns that in-band threshold before recursion; this is required
/// bound sanitation rather than mate-distance pruning.
#[test]
fn out_of_band_windows_do_not_leak_into_returned_scores() {
    chess::init::init_globals();

    let out_of_band_alpha = Score::from_i16(20_100);
    let out_of_band_beta = Score::from_i16(20_101);
    assert_eq!(Score::mate(0).child_bound(), out_of_band_beta);
    assert!(!out_of_band_alpha.is_node_score());
    assert!(!out_of_band_beta.is_node_score());

    for depth in [0 as Depth, 1, 2] {
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let (sender, _events) = unbounded();
        let mut search = Search::with_events(
            Position::from_fen("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61").unwrap(),
            &flag,
            Deadlines::none(),
            None,
            &table,
            sender,
            None,
        );

        let value = search
            .search::<Master, NonPv>(out_of_band_alpha, out_of_band_beta, depth, 0)
            .expect("an uncancelled search must produce a score");

        assert!(
            value.is_node_score(),
            "depth {depth} returned {value:?}, outside the node score band",
        );
        // The parent's view has to be well formed too, since that is what reaches `Display`.
        assert!(value.neg().inc_mate().is_node_score());
    }
}

/// The same window, entered directly at quiescence. Quiescence is where the excursion used to
/// compound, because it had no window normalization to absorb an out-of-band bound and it
/// returns `alpha` and `beta` themselves as fail-soft scores.
#[test]
fn quiescence_clamps_out_of_band_windows_into_the_node_score_band() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let (sender, _events) = unbounded();
    let mut search = Search::with_events(
        Position::from_fen("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61").unwrap(),
        &flag,
        Deadlines::none(),
        None,
        &table,
        sender,
        None,
    );

    let value = search
        .quiesce::<Master, NonPv>(Score::from_i16(20_100), Score::from_i16(20_101), 0)
        .expect("an uncancelled quiescence search must produce a score");

    assert_eq!(value, Score::mate(1));
    assert!(value.is_node_score());
}

#[test]
fn search_emits_typed_current_move_events() {
    chess::init::init_globals();

    let mut position = Position::start_pos();
    let current_move = position.make_uci_move("e2e4").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let (sender, events) = unbounded();
    let search = Search::with_events(
        position,
        &flag,
        Deadlines::none(),
        None,
        &table,
        sender,
        None,
    );

    search.emit_current_move(7, &current_move, 4);

    assert_eq!(
        events.recv().unwrap(),
        SearchEvent::CurrentMove(CurrentMove {
            depth: 7,
            current_move,
            number: 4,
        })
    );
}

#[test]
fn typed_api_cancels_running_search() {
    chess::init::init_globals();

    let engine = SearchEngine::new(1);
    let search = engine.start(Position::start_pos(), SearchLimit::Infinite);
    let events = search.events().clone();
    search
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("search should produce progress before cancellation");
    search.cancel();
    let outcome = search.wait();

    assert!(outcome.was_cancelled());
    assert!(outcome.result().unwrap().depth >= 1);
    assert!(outcome.result().unwrap().best_move.is_some());
    assert!(events.try_iter().all(|event| match event {
        SearchEvent::Progress(progress) => {
            progress.principal_variation.len() <= usize::from(progress.depth)
        }
        SearchEvent::CurrentMove(_) => true,
    }));
}

#[test]
fn mid_subtree_abort_keeps_the_last_completed_iteration() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);

    // Measure the deterministic depth-one work, then stop a fresh search in the first child
    // of the candidate depth-two root. The root itself is the first new node and its child is
    // the second, so this threshold proves that a move was made and a subtree was entered.
    let baseline_table = Table::new(16);
    let mut baseline = Search::new(position.clone(), &flag, None, &baseline_table);
    let expected = baseline.run::<Master>(1).unwrap();
    let expected_pv = baseline.pvt.pv().copied().collect::<Vec<_>>();
    let completed_iteration_nodes = baseline.trace.all_nodes_visited();
    let abort_after = completed_iteration_nodes + 2;

    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);
    search.abort_after_nodes = Some(abort_after);
    let result = search.run::<Master>(3).unwrap();

    assert_eq!(result, expected);
    assert!(search.trace.all_nodes_visited() >= abort_after);
    assert_eq!(search.pvt.pv().copied().collect::<Vec<_>>(), expected_pv);

    // The aborted depth-two root must not replace the completed depth-one root entry.
    let root_entry = table
        .probe(position.zobrist().0)
        .expect("the completed depth-one root must still be in the table");
    assert_eq!(root_entry.depth(), 1);
    assert_eq!(
        root_entry
            .mov()
            .expect("the root entry carries its best move")
            .to_move(&position),
        expected.best_move.unwrap()
    );
}

#[test]
fn aborted_child_cannot_score_or_write_its_parent() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let start_zob = position.zobrist();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);

    // Permit the test abort immediately and fire it in the first child: the root consumes one
    // node, makes a move, and the recursive search consumes the second node before stopping.
    search.min_search_complete = true;
    search.pvt = PVTable::new(2);
    search.abort_after_nodes = Some(2);

    let result = search.search::<Master, Root>(Score::INF_N, Score::INF_P, 2, 0);

    assert_eq!(result, None, "an aborted child must not yield a score");
    assert_eq!(search.trace.all_nodes_visited(), 2);
    assert_eq!(search.pos.zobrist(), start_zob, "the root move is restored");
    assert!(
        search.pvt.pv().next().is_none(),
        "an aborted child must not become the principal move"
    );
    assert!(
        table.probe(position.zobrist().0).is_none(),
        "an ancestor whose child aborted must not write a TT entry"
    );
}

#[test]
fn zero_time_limit_still_returns_a_legal_move() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let engine = SearchEngine::new(1);
    let search = engine.start(
        position.clone(),
        SearchLimit::Time(TimeBudget::fixed(Duration::ZERO)),
    );
    let outcome = search.wait();

    // A zero budget must never forfeit: the guaranteed-minimum ply completes and yields a
    // legal move rather than an absent result (which UCI would emit as `bestmove 0000`).
    assert!(matches!(outcome, SearchOutcome::Completed(_)));
    let result = outcome.result().expect("a legal move must be returned");
    assert!(result.depth >= 1);
    let best_move = result
        .best_move
        .expect("non-terminal position has a legal move");
    assert!(
        position.valid_move(&best_move),
        "returned move must be legal"
    );
}

#[test]
fn near_zero_time_budget_completes_the_guaranteed_ply() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let engine = SearchEngine::new(1);
    let search = engine.start(
        position.clone(),
        SearchLimit::Time(TimeBudget::fixed(Duration::from_nanos(1))),
    );
    let result = search.wait().result().cloned();

    let result = result.expect("near-zero budget must still return a legal move");
    assert!(result.depth >= 1);
    assert!(position.valid_move(&result.best_move.unwrap()));
}

#[test]
fn the_time_deadline_is_suppressed_until_the_first_ply_completes() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    // The deadline has already elapsed, and the root fallback is established, but only the
    // completed first ply may release the time-based abort.
    let mut search = Search::new(position, &flag, Some(Instant::now()), &table);
    search.root_fallback_ready = true;

    assert!(!search.stopping());

    search.min_search_complete = true;
    assert!(search.stopping());
}

#[test]
fn cancellation_is_suppressed_only_until_the_root_fallback_exists() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(true);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    // Nothing legal has been recorded yet, so cancellation cannot abort: doing so would forfeit
    // with `bestmove 0000`.
    assert!(!search.stopping());

    // The fallback alone releases the cancellation flag. Unlike the time deadline, it does not
    // wait for the first ply, so no unbounded quiescence tree stands between `stop` and the
    // abort.
    search.establish_root_fallback();
    assert!(!search.min_search_complete);
    assert!(search.stopping());
}

#[test]
fn cancellation_is_not_throttled_with_the_deadline_clock() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(
        Position::start_pos(),
        &flag,
        Some(Instant::now() + Duration::from_secs(60)),
        &table,
    );
    search.establish_root_fallback();
    search.min_search_complete = true;

    // The deadline sample taken here throttles subsequent clock reads, but it must not defer
    // the cancellation flag: the very next check at the same node has to observe the stop.
    assert!(!search.stopping());
    flag.store(true, Ordering::Relaxed);
    assert!(
        search.stopping(),
        "cancellation must be read at the same node"
    );
}

#[test]
fn expired_deadline_stays_latched_at_the_same_node() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(Position::start_pos(), &flag, Some(Instant::now()), &table);
    search.min_search_complete = true;

    assert!(search.stopping(), "the elapsed deadline must stop search");
    assert!(
        search.stopping(),
        "deadline expiry must remain latched during same-node unwind checks"
    );
}

#[test]
fn time_limited_search_honors_the_budget_after_the_guaranteed_ply() {
    chess::init::init_globals();

    let budget = Duration::from_millis(20);
    let started = Instant::now();
    let engine = SearchEngine::new(1);
    let search = engine.start(
        Position::start_pos(),
        SearchLimit::Time(TimeBudget::fixed(budget)),
    );
    let outcome = search.wait();
    let elapsed = started.elapsed();

    // The search returns of its own accord (the deadline aborts it) rather than running to the
    // maximum depth, and it still reports a completed legal move.
    assert!(matches!(outcome, SearchOutcome::Completed(_)));
    let result = outcome.result().expect("a legal move must be returned");
    assert!(result.depth >= 1);
    // Release deadline checks are at most 8 nodes apart (one node in debug builds). The
    // additional 100 ms allows for a slow or descheduled CI worker while still catching a
    // missed or excessively coarse sample.
    assert!(
        elapsed <= budget + Duration::from_millis(100),
        "{budget:?} search exceeded deadline tolerance: {elapsed:?}"
    );
}

/// A budget that permits an extension must still be bounded by its hard half. This is the
/// property a real game depends on: the soft limit is what the search *plans* to spend, so
/// only the hard limit stands between an unstable position and a flag.
#[test]
fn an_extendable_budget_is_still_bounded_by_its_hard_half() {
    chess::init::init_globals();

    // A position sharp enough that the root move and score genuinely move between iterations,
    // so the instability extension is live rather than hypothetical.
    let position =
        Position::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1")
            .expect("valid FEN");
    let budget = TimeBudget::new(Duration::from_millis(20), Duration::from_millis(60));

    let started = Instant::now();
    let engine = SearchEngine::new(1);
    let outcome = engine.start(position, SearchLimit::Time(budget)).wait();
    let elapsed = started.elapsed();

    assert!(outcome.result().is_some_and(|r| r.best_move.is_some()));
    // The same tolerance the fixed-budget deadline test uses, for the same reason: deadline
    // samples are up to eight nodes apart and a descheduled worker must not fail the test.
    assert!(
        elapsed <= budget.hard() + Duration::from_millis(100),
        "search ran to {elapsed:?}, past its {:?} hard limit",
        budget.hard()
    );
}

/// The prediction must not fire until it has something to predict from. Gating on a guess
/// would risk declining the guaranteed first ply, which is what makes a legal `bestmove`
/// unconditional.
#[test]
fn no_iteration_is_declined_before_two_have_been_measured() {
    let mut cost = IterationCost::default();
    assert_eq!(cost.predict_next(), None);

    cost.record(Duration::from_millis(10));
    assert_eq!(cost.predict_next(), None);

    cost.record(Duration::from_millis(30));
    assert!(cost.predict_next().is_some());
}

/// The estimate is the observed growth between the last two iterations, applied to the last —
/// not a constant, because a forcing position and an open middlegame grow at quite different
/// rates and a constant is wrong for one of them.
#[test]
fn the_prediction_extrapolates_the_measured_growth() {
    let mut cost = IterationCost::default();
    cost.record(Duration::from_millis(10));
    cost.record(Duration::from_millis(30));

    // Growth of 3x, applied to the 30ms iteration.
    assert_eq!(cost.predict_next(), Some(Duration::from_millis(90)));
}

/// A single anomalous iteration — a root fail-high, or a transposition table that happened to
/// hold the whole line — must degrade the prediction rather than dominate it.
#[test]
fn an_outlying_growth_ratio_is_clamped_to_a_plausible_range() {
    let mut shrinking = IterationCost::default();
    shrinking.record(Duration::from_millis(100));
    shrinking.record(Duration::from_millis(10));
    assert_eq!(
        shrinking.predict_next(),
        Some(Duration::from_millis(10).mul_f64(MIN_BRANCHING_FACTOR)),
        "an iteration cheaper than its predecessor must not predict a cheaper one still"
    );

    let exploding = {
        let mut cost = IterationCost::default();
        cost.record(Duration::from_millis(1));
        cost.record(Duration::from_millis(500));
        cost
    };
    assert_eq!(
        exploding.predict_next(),
        Some(Duration::from_millis(500).mul_f64(MAX_BRANCHING_FACTOR))
    );
}

/// The opening iterations of a search finish in microseconds, where the measurement is
/// dominated by clock resolution and scheduling. Extrapolating from one would decline
/// iterations on the strength of noise.
#[test]
fn an_unmeasurably_short_iteration_yields_no_prediction() {
    let mut cost = IterationCost::default();
    cost.record(MIN_MEASURABLE_ITERATION - Duration::from_nanos(1));
    cost.record(Duration::from_millis(50));

    assert_eq!(cost.predict_next(), None);
}

/// A settled root that has not yet held long enough to contract spends what it was allotted and no
/// more. Anything above 1 would make the extension the normal case, which is the same as having
/// allotted more in the first place; anything below would contract before stability is established.
#[test]
fn a_stable_root_asks_for_no_extension() {
    assert_eq!(stability_scale(false, 0, 0), 1.0);
    // A rising score — a negative drop — is the search finding more than it expected, not a
    // reason to distrust the move it is about to play.
    assert_eq!(stability_scale(false, -400, 0), 1.0);
}

#[test]
fn a_changed_best_move_or_a_falling_score_asks_for_an_extension() {
    assert!(stability_scale(true, 0, 0) > 1.0);
    assert!(stability_scale(false, 50, 0) > 1.0);

    // The two compound, and a larger drop asks for more than a smaller one.
    assert!(stability_scale(true, 300, 0) > stability_scale(true, 50, 0));
    assert!(stability_scale(true, 50, 0) > stability_scale(false, 50, 0));

    // A collapsing evaluation cannot ask without bound.
    assert_eq!(
        stability_scale(true, 30_000, 0),
        1.0 + BEST_MOVE_CHANGE_EXTENSION + MAX_SCORE_DROP_EXTENSION
    );
}

/// The two directions never compete. The caller resets the settled streak to zero the instant the
/// root move changes or the score leaves the flat margin, so an unsettled iteration always reaches
/// `stability_scale` with `stable_iterations == 0` and extends exactly as it did before contraction
/// existed; a settled iteration always arrives with a nonzero streak and contracts. A sub-margin
/// wobble — the ordinary texture of a flat search, which does not hold its score perfectly still —
/// is the settled case, not a fall, and must not veto the contraction.
#[test]
fn settled_contracts_and_unsettled_extends_without_competing() {
    // Unsettled (streak reset to zero by the caller): extends.
    assert!(stability_scale(true, 0, 0) > 1.0);
    assert!(stability_scale(false, 50, 0) > 1.0);

    // Settled and held well past the onset: contracts, even though the flat score wobbled a few
    // centipawns either way — the `score_drop` an unconditional extension would have fired on.
    assert!(stability_scale(false, 4, 20) < 1.0);
    assert!(stability_scale(false, -4, 20) < 1.0);
}

/// Contraction only begins once the position has been settled for several consecutive iterations.
/// A position that has only just gone quiet is not yet evidence the answer has stopped moving, so
/// the early plies keep their full planned spend.
#[test]
fn contraction_waits_for_a_streak_then_decreases_monotonically() {
    // Up to and including the onset, a settled position still spends exactly its planned share.
    for streak in 0..=STABILITY_CONTRACTION_ONSET {
        assert_eq!(
            stability_scale(false, 0, streak),
            1.0,
            "contracted at streak {streak}, before the onset"
        );
    }

    // Past the onset each further settled iteration removes another step, so the multiplier is
    // strictly decreasing until it reaches the floor.
    let just_past = stability_scale(false, 0, STABILITY_CONTRACTION_ONSET + 1);
    assert!(just_past < 1.0);
    assert!(stability_scale(false, 0, STABILITY_CONTRACTION_ONSET + 2) < just_past);
}

/// The contraction is bounded: a position settled for an arbitrarily long time never pulls its
/// planned spend below the documented floor, so a genuine resolution is never starved of depth.
#[test]
fn contraction_never_falls_below_the_floor() {
    assert_eq!(stability_scale(false, 0, u32::MAX), MIN_STABILITY_SCALE);
    assert!(stability_scale(false, 0, 10_000) >= MIN_STABILITY_SCALE);
}

/// Without a soft limit there is nothing to decline against, so an untimed search — depth,
/// nodes, or infinite — must reach every iteration it is asked for.
#[test]
fn an_untimed_search_never_declines_an_iteration() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let search = Search::new(Position::start_pos(), &flag, None, &table);

    let mut cost = IterationCost::default();
    cost.record(Duration::from_secs(1));
    cost.record(Duration::from_secs(10));

    assert!(search.next_iteration_fits(&cost, 1.0));
}

/// The point of the whole prediction: an iteration whose expected cost overruns the budget is
/// declined, because an aborted iteration is discarded whole and the time spent on it buys
/// nothing.
#[test]
fn an_iteration_predicted_to_overrun_the_budget_is_declined() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(Position::start_pos(), &flag, None, &table);

    // Half a second of budget remaining, against an iteration expected to cost three.
    let start = Instant::now();
    search.soft_limit = Some(SoftLimit {
        start,
        budget: Duration::from_millis(500),
    });
    search.stop_time = Some(start + Duration::from_millis(500));

    let mut cost = IterationCost::default();
    cost.record(Duration::from_millis(10));
    cost.record(Duration::from_secs(1));

    assert!(!search.next_iteration_fits(&cost, 1.0));

    // The same prediction against a budget that comfortably accommodates it.
    search.soft_limit = Some(SoftLimit {
        start,
        budget: Duration::from_secs(60),
    });
    search.stop_time = Some(start + Duration::from_secs(60));
    assert!(search.next_iteration_fits(&cost, 1.0));
}

/// Instability is what buys the extra iteration: the same prediction that does not fit the
/// planned spend fits once the position has earned the extension.
#[test]
fn instability_buys_an_iteration_the_planned_spend_could_not() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(Position::start_pos(), &flag, None, &table);

    let start = Instant::now();
    search.soft_limit = Some(SoftLimit {
        start,
        budget: Duration::from_millis(100),
    });
    search.stop_time = Some(start + Duration::from_millis(400));

    // Predicted at 200ms: twice the planned spend, half the hard limit.
    let mut cost = IterationCost::default();
    cost.record(Duration::from_millis(50));
    cost.record(Duration::from_millis(100));
    assert_eq!(cost.predict_next(), Some(Duration::from_millis(200)));

    assert!(!search.next_iteration_fits(&cost, 1.0));
    assert!(search.next_iteration_fits(&cost, 3.0));
}

/// The contraction direction is the mirror image: a next iteration the full planned spend would
/// have admitted is declined once the position has settled enough to contract below optimum. This
/// is the whole point of the lever — releasing the unspent clock to later moves on a settled
/// position rather than searching for a better move that is not there.
#[test]
fn contraction_declines_an_iteration_the_planned_spend_would_have_admitted() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(Position::start_pos(), &flag, None, &table);

    let start = Instant::now();
    // A generous hard limit, so only the soft-limit scaling can decide the outcome here.
    search.soft_limit = Some(SoftLimit {
        start,
        budget: Duration::from_millis(100),
    });
    search.stop_time = Some(start + Duration::from_secs(60));

    // Predicted at 90ms: inside the 100ms planned spend, so it fits at the neutral scale.
    let mut cost = IterationCost::default();
    cost.record(Duration::from_millis(40));
    cost.record(Duration::from_millis(60));
    assert_eq!(cost.predict_next(), Some(Duration::from_millis(90)));

    assert!(search.next_iteration_fits(&cost, 1.0));
    // Contracted to half the planned spend, the same prediction no longer fits.
    assert!(!search.next_iteration_fits(&cost, MIN_STABILITY_SCALE));
}

/// However unstable the position, an iteration that would run past the hard deadline is still
/// declined. The extension draws on time the clock holds; it does not create any.
#[test]
fn no_extension_starts_an_iteration_that_would_pass_the_hard_deadline() {
    chess::init::init_globals();

    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(Position::start_pos(), &flag, None, &table);

    let start = Instant::now();
    search.soft_limit = Some(SoftLimit {
        start,
        budget: Duration::from_millis(100),
    });
    search.stop_time = Some(start + Duration::from_millis(150));

    let mut cost = IterationCost::default();
    cost.record(Duration::from_millis(50));
    cost.record(Duration::from_millis(100));

    // A scale large enough that the soft deadline alone would admit the 200ms prediction many
    // times over. The hard deadline is what refuses it.
    assert!(!search.next_iteration_fits(&cost, 100.0));
}

#[test]
fn the_node_limit_is_suppressed_until_the_first_ply_completes() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    // The budget is already spent (zero nodes), and the root fallback is established, but only
    // the completed first ply may release the node-based abort — so a budget too small to
    // finish a ply returns a searched move rather than the unsearched fallback.
    let mut search = Search::new(position, &flag, None, &table);
    search.node_limit = Some(0);
    search.root_fallback_ready = true;

    assert!(!search.stopping());

    search.min_search_complete = true;
    assert!(search.stopping());
}

#[test]
fn cancellation_under_a_node_limit_is_not_gated_on_the_budget() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(true);
    let table = Table::new(1);
    // A budget far larger than anything the search will visit, and no completed first ply. The
    // node limit is checked only after the cancellation flag, and cancellation aborts as soon
    // as the root fallback exists — so a `stop` must not wait for the budget or the first ply.
    let mut search = Search::new(position, &flag, None, &table);
    search.node_limit = Some(u64::MAX);
    search.establish_root_fallback();
    assert!(!search.min_search_complete);

    assert!(
        search.stopping(),
        "cancellation must abort without waiting for the node budget"
    );
}

#[test]
fn a_node_budget_below_one_ply_still_returns_a_legal_move() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let engine = SearchEngine::new(1);
    // One node cannot complete a ply, but the guaranteed-minimum search must run regardless,
    // exactly as it does under a zero time budget.
    let search = engine.start(position.clone(), SearchLimit::Nodes(1));
    let outcome = search.wait();

    assert!(matches!(outcome, SearchOutcome::Completed(_)));
    let result = outcome.result().expect("a legal move must be returned");
    assert!(result.depth >= 1);
    let best_move = result
        .best_move
        .expect("non-terminal position has a legal move");
    assert!(
        position.valid_move(&best_move),
        "returned move must be legal"
    );
}

#[test]
fn a_node_budget_exhausted_mid_iteration_keeps_the_last_completed_iteration() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);

    // Measure the guaranteed first ply, then set a budget a couple of nodes beyond it. The node
    // limit is suppressed until that ply completes, so the budget can only bind partway through
    // the second iteration.
    let baseline_table = Table::new(16);
    let mut baseline = Search::new(position.clone(), &flag, None, &baseline_table);
    let expected = baseline.run::<Master>(1).unwrap();
    let first_ply_nodes = baseline.trace.all_nodes_visited();
    let budget = (first_ply_nodes + 2) as u64;

    let table = Table::new(16);
    let mut search = Search::new(position.clone(), &flag, None, &table);
    search.node_limit = Some(budget);
    let result = search.run::<Master>(MAX_DEPTH).unwrap();

    // The aborted second iteration is discarded; the completed first ply is what is returned.
    assert_eq!(result, expected);
    assert!(search.trace.all_nodes_visited() >= budget as usize);
}

#[test]
fn a_node_limited_search_is_reproducible_across_runs() {
    chess::init::init_globals();

    // A position with a large quiescence tree, so a mid-search abort lands deep in the
    // recursion where any nondeterminism would surface.
    let position = Position::from_fen(QUIESCENCE_HEAVY_FEN).unwrap();
    let budget = 5_000;

    // Each run starts from a fresh table and cancellation flag, so nothing but the deterministic
    // node visitation decides where the budget binds.
    let run = || {
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);
        search.node_limit = Some(budget);
        let result = search.run::<Master>(MAX_DEPTH).unwrap();
        (result, search.trace.all_nodes_visited())
    };

    let (first, first_nodes) = run();
    let (second, second_nodes) = run();

    assert_eq!(
        first, second,
        "the same build, position and budget must return the same move and score"
    );
    assert_eq!(
        first_nodes, second_nodes,
        "node visitation must be identical across runs"
    );

    // The budget genuinely bound rather than the search exhausting the depth first.
    assert!(first_nodes >= budget as usize);
    assert!(
        first.depth < MAX_DEPTH,
        "a {budget}-node budget cannot reach the maximum depth"
    );
}

#[test]
fn immediate_cancellation_returns_a_legal_move() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let engine = SearchEngine::new(1);
    let search = engine.start(position.clone(), SearchLimit::Infinite);
    search.cancel();
    let outcome = search.wait();

    // Cancellation may win the race before any root move is searched. The fallback means the
    // result is nonetheless a legal move rather than the `bestmove 0000` forfeit.
    assert!(outcome.was_cancelled());
    let best_move = outcome
        .result()
        .expect("a cancelled search must still report the root fallback")
        .best_move
        .expect("non-terminal position has a legal move");
    assert!(position.valid_move(&best_move));
}

/// A position whose depth-1 quiescence tree is large enough that searching it is plainly
/// distinguishable from not searching it.
const QUIESCENCE_HEAVY_FEN: &str =
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

#[test]
fn cancellation_stops_the_first_iteration_without_searching_it() {
    chess::init::init_globals();

    let position = Position::from_fen(QUIESCENCE_HEAVY_FEN).unwrap();
    let table = Table::new(1);

    // The same first iteration, uncancelled, is the baseline this is measured against.
    let running = AtomicBool::new(false);
    let mut baseline = Search::new(position.clone(), &running, None, &table);
    let searched = baseline.run::<Master>(1).expect("first ply completes");
    let searched_nodes = baseline.trace.all_nodes_visited();
    assert!(searched_nodes > 1_000, "baseline must be a real search");

    // With cancellation already set, the search returns without visiting a single node: it
    // never enters the depth-1 quiescence tree, whose size has no practically small bound.
    // This is the deterministic form of "an explicit stop is honored promptly".
    let cancelled = AtomicBool::new(true);
    let mut search = Search::new(position.clone(), &cancelled, None, &table);
    let result = search
        .run::<Master>(1)
        .expect("cancellation must still yield the root fallback");

    assert_eq!(search.trace.all_nodes_visited(), 0);
    assert_eq!(result.depth, 0, "no iteration completed");
    let best_move = result.best_move.expect("root has legal moves");
    assert!(
        position.valid_move(&best_move),
        "fallback move must be legal"
    );
    assert!(searched.best_move.is_some());
}

#[test]
fn the_root_fallback_tracks_the_best_searched_root_move() {
    chess::init::init_globals();

    let position = Position::from_fen(QUIESCENCE_HEAVY_FEN).unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    let result = search.run::<Master>(2).expect("search completes");

    // Cancelling mid-ply reports this move, not the arbitrary first generated one: the fallback
    // is upgraded as each root move is fully searched.
    assert_eq!(search.root_fallback, result.best_move);
}

#[test]
fn cancelled_terminal_root_reports_no_move() {
    chess::init::init_globals();

    // Checkmate: there is no legal move to fall back to, so cancellation must not invent one.
    let position = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
    let engine = SearchEngine::new(1);
    let search = engine.start(position, SearchLimit::Infinite);
    search.cancel();
    let outcome = search.wait();

    assert!(outcome
        .result()
        .is_none_or(|result| result.best_move.is_none()));
    assert_eq!(
        crate::info::format_search_outcome(&outcome),
        "bestmove 0000"
    );
}

#[test]
fn terminal_position_returns_score_without_a_best_move() {
    chess::init::init_globals();

    let position = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
    let engine = SearchEngine::new(1);
    let outcome = engine.start(position, SearchLimit::Depth(1)).wait();
    let result = outcome.result().unwrap();

    assert!(matches!(outcome, SearchOutcome::Completed(Some(_))));
    assert_eq!(result.depth, 1);
    assert_eq!(result.best_move, None);
}

#[test]
fn typed_api_supports_time_limits() {
    chess::init::init_globals();

    let engine = SearchEngine::new(1);
    let search = engine.start(
        Position::start_pos(),
        SearchLimit::Time(TimeBudget::fixed(Duration::from_millis(10))),
    );
    let outcome = search.wait();

    assert!(matches!(outcome, SearchOutcome::Completed(_)));
}

/// The self-play game, replayed verbatim from the FastChess record, whose final position made
/// seaborg report `info depth 4 ... score mate -2 ... pv d7f8 g6a6 f8g6 c5f8` — a line whose
/// fourth ply `c5f8` is illegal. The move list is used rather than the equivalent FEN because
/// the repetition history it builds up is part of what the search sees.
const ILLEGAL_MATE_PV_GAME: &str = "a2a3 a7a6 b2b3 a6a5 c2c3 b7b6 d2d3 b6b5 e2e3 a5a4 b3a4 \
    b5a4 f2f3 c7c6 g2g3 c6c5 h2h3 d7d6 c3c4 d6d5 c4d5 d8d5 d3d4 c5d4 e3d4 e7e6 g3g4 e6e5 d4e5 \
    d5a5 e1f2 a5e5 a1a2 f7f6 a2e2 f8c5 f2e1 e5e2 f1e2 a8a5 c1d2 a5a7 d1c2 a7b7 b1c3 e8d8 c3b5 \
    b8d7 d2a5 c5b6 c2a4 b6a5 a4a5 d8e7 f3f4 g7g6 g4g5 f6g5 a5c3 g8f6 f4g5 h7h6 c3e3 e7f8 e3c1 \
    f8e7 c1e3 e7f8 e3c3 f8e7 c3b4 e7e6 e2c4 e6e5 b4b2 e5f4 g5f6 h8e8 g1e2 f4g5 b2c1 g5h5 b5d6 \
    e8e5 a3a4 b7c7 d6f7 g6g5 h3h4 c8b7 h1h2 e5e4 h4g5 h5g6 h2h6 g6f5 f7d6 f5e5 d6e4 c7c4 c1c4 \
    b7e4 f6f7 e4g6 h6g6";

/// Positions whose reported PVs are checked for legality: the pinned self-play reproduction,
/// two opening positions, and the mate and tactical positions from the search suite, which are
/// the mate-scored/shallow lines the defect surfaced on.
fn pv_legality_positions() -> Vec<(String, Position)> {
    let mut positions = vec![(
        format!("startpos moves {ILLEGAL_MATE_PV_GAME}"),
        position_after(ILLEGAL_MATE_PV_GAME),
    )];

    for fen in ["rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"]
        .into_iter()
        .chain(suite().iter().map(|entry| entry.0))
    {
        positions.push((fen.to_owned(), Position::from_fen(fen).unwrap()));
    }

    positions
}

fn position_after(moves: &str) -> Position {
    let mut position = Position::start_pos();

    for uci in moves.split_whitespace() {
        position
            .make_uci_move(uci)
            .unwrap_or_else(|| panic!("{uci} should be legal in {}", position.to_fen()));
    }

    position
}

/// Replays a reported principal variation exactly as a UCI GUI would: each move must be legal
/// in the position reached by playing the preceding PV moves.
fn assert_pv_is_legal(label: &str, root: &Position, depth: u8, pv: &[Move]) {
    let mut position = root.clone();

    for (index, mov) in pv.iter().enumerate() {
        let uci = mov.to_uci_string();
        assert!(
            position.make_uci_move(&uci).is_some(),
            "illegal PV move at ply {} ({uci}) of depth-{depth} pv [{}] \
             reported for `{label}`; illegal in {}",
            index + 1,
            pv.iter()
                .map(|m| m.to_uci_string())
                .collect::<Vec<_>>()
                .join(" "),
            position.to_fen(),
        );
    }
}

/// Collects every principal variation the search reports over the typed event channel.
fn reported_pvs(engine: &SearchEngine, root: &Position, depth: u8) -> Vec<(u8, Vec<Move>)> {
    let search = engine.start(root.clone(), SearchLimit::Depth(depth));
    let events = search.events().clone();
    let _ = search.wait();

    events
        .try_iter()
        .filter_map(|event| match event {
            SearchEvent::Progress(progress) => Some((progress.depth, progress.principal_variation)),
            SearchEvent::CurrentMove(_) => None,
        })
        .collect()
}

/// Every move of every reported PV must be legal in the position it is played from. Regression
/// for illegal deep PV plies spliced up from a stale sibling row or published by a fail-high
/// node, which produced `pv d7f8 g6a6 f8g6 c5f8` scored `mate -2` in self-play.
#[test]
fn reported_principal_variations_are_legal() {
    chess::init::init_globals();

    for (label, root) in pv_legality_positions() {
        // A fresh engine per position keeps the transposition table cold; the second pass
        // reuses the warm table, which is the state self-play actually reports from.
        let engine = SearchEngine::new(1);

        for _ in 0..2 {
            for depth in 1..=6 {
                for (reported_depth, pv) in reported_pvs(&engine, &root, depth) {
                    assert_pv_is_legal(&label, &root, reported_depth, &pv);
                }
            }
        }
    }
}

/// An extended subtree runs deeper than the horizon the PV table was sized for, so it reaches
/// plies the table has no row for.
///
/// While per-ply state was derived by subtracting remaining depth from the iteration depth,
/// this was fatal rather than merely untidy: the subtraction underflowed, and the wrapped
/// result indexed the PV table far out of bounds. Indexing by ply makes the deep plies simply
/// fall outside the table, so the search completes and reports the part of the line the table
/// does cover — every move of which must still be legal.
#[test]
fn a_node_searched_past_the_nominal_horizon_still_reports_a_legal_pv() {
    chess::init::init_globals();

    // The horizon the PV table is sized for, and the greater depth the node is actually
    // searched to, standing in for what an extension would produce.
    const NOMINAL: u8 = 3;
    const EXTENDED: Depth = 6;

    for (label, root) in pv_legality_positions() {
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(root.clone(), &flag, None, &table);
        search.pvt = PVTable::new(NOMINAL);

        search
            .search::<Master, Root>(Score::INF_N, Score::INF_P, EXTENDED, 0)
            .expect("an uncancelled search must produce a score");

        let pv = search.pvt.pv().copied().collect::<Vec<_>>();
        assert!(
            pv.len() <= NOMINAL as usize,
            "a node past the horizon wrote a row the table does not cover: \
             reported {} moves for `{label}`",
            pv.len(),
        );
        assert_pv_is_legal(&label, &root, NOMINAL, &pv);
    }
}

/// A reduction can take remaining depth past zero in a single step, so depth is signed and the
/// handover to quiescence triggers on "at or below zero" rather than on an exact zero. An
/// unsigned depth would have wrapped to a near-infinite one instead.
#[test]
fn a_depth_reduced_below_zero_hands_over_to_quiescence() {
    chess::init::init_globals();

    let position =
        Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4")
            .unwrap();

    for depth in [0 as Depth, -1, -7] {
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(position.clone(), &flag, None, &table);
        let searched = search
            .search::<Master, Pv>(Score::INF_N, Score::INF_P, depth, 0)
            .expect("an uncancelled search must produce a score");

        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut quiescence = Search::new(position.clone(), &flag, None, &table);
        let expected = quiescence
            .quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0)
            .expect("an uncancelled quiescence search must produce a score");

        assert_eq!(
            searched, expected,
            "depth {depth} did not hand over to quiescence",
        );
    }
}

/// A quiet move used as a killer at a shallow depth is just as likely to refute the same ply on
/// the next, deeper iteration, so the table is not cleared between iterative-deepening
/// iterations. A killer seeded past the reach of a shallow search survives every iteration of
/// that search, proving the deepening loop preserves it rather than rebuilding from empty.
#[test]
fn killers_persist_across_iterative_deepening_iterations() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    // A ply far deeper than a depth-4 main search can reach, so the search never overwrites this
    // slot itself and its survival can only mean the deepening loop left it in place.
    let deep_ply = 20;
    let seeded = Move::build(Square::E2, Square::E4, None, MoveType::QUIET);
    search.kt.store(seeded, deep_ply);

    // `iterative_deepening` runs every iteration but, unlike `run`, does not reset afterwards, so
    // the table can be inspected in the state the final iteration left it.
    search.iterative_deepening::<Master>(4);

    assert_eq!(search.kt.slot_of(deep_ply, seeded), Some(0));
}

/// Killers are scoped to a single search. A refutation learned for one position must not order
/// moves in a later, unrelated search on the same worker, so `run` clears the table when it
/// finishes and the next search starts from empty.
#[test]
fn a_new_search_run_starts_from_an_empty_killer_table() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table = Table::new(1);
    let mut search = Search::new(position, &flag, None, &table);

    let deep_ply = 20;
    let seeded = Move::build(Square::E2, Square::E4, None, MoveType::QUIET);
    search.kt.store(seeded, deep_ply);

    search.run::<Master>(4);

    assert_eq!(search.kt.slot_of(deep_ply, seeded), None);
}

/// Each search owns its own killer table, which is the ownership a Lazy SMP worker relies on: one
/// worker's refutations never appear in another's ordering. Two independent searches storing at
/// the same ply keep their tables separate.
#[test]
fn separate_searches_own_independent_killer_tables() {
    chess::init::init_globals();

    let position = Position::start_pos();
    let flag = AtomicBool::new(false);
    let table_a = Table::new(1);
    let table_b = Table::new(1);
    let mut search_a = Search::new(position.clone(), &flag, None, &table_a);
    let search_b = Search::new(position, &flag, None, &table_b);

    let killer = Move::build(Square::E2, Square::E4, None, MoveType::QUIET);
    search_a.kt.store(killer, 3);

    assert_eq!(search_a.kt.slot_of(3, killer), Some(0));
    assert_eq!(search_b.kt.slot_of(3, killer), None);
}

/// Static-exchange pruning must not change the result of a search whose answer is forced: with
/// the cuts switched off, a fixed-depth search of each position returns exactly the score and
/// best move it returns with them on. The suite is chosen so the correct move, or the proof of
/// the correct move, hinges on a *losing* capture — a queen taking a defended pawn to mate, and
/// a mate whose forcing line sacrifices material — which is precisely what these cuts are most at
/// risk of discarding. That the answers are identical either way is the guard-correctness
/// contract: where the cuts fire they only skip work the full search would have thrown away.
#[test]
fn see_pruning_leaves_forced_results_unchanged() {
    chess::init::init_globals();

    let positions = [
        // Qxg7# — the only mate, and it is a queen-for-pawn capture (SEE deeply negative) that
        // survives the cut solely because it gives check.
        ("6k1/5ppp/7Q/5N2/8/8/8/6K1 w - - 0 1", 2),
        // A mate in three whose forcing sequence gives up material on the way in.
        (
            "r2qkb1r/pp2nppp/3p4/2pNN1B1/2BnP3/3P4/PPP2PPP/R2bK2R w KQkq - 1 1",
            6,
        ),
        // A mate in seven whose proof rests on sacrificial captures deep in the tree; the
        // quiescence cuts must keep those checking captures searchable rather than revert it to
        // a bare material score.
        ("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61", 6),
    ];

    for (fen, depth) in positions {
        let position = Position::from_fen(fen).unwrap();
        let flag = AtomicBool::new(false);

        let pruned_table = Table::new(16);
        let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
        let pruned_result = pruned.run::<Master>(depth).unwrap();

        let plain_table = Table::new(16);
        let mut plain = Search::new(position, &flag, None, &plain_table);
        plain.see_pruning_disabled = true;
        let plain_result = plain.run::<Master>(depth).unwrap();

        assert_eq!(
            pruned_result.score, plain_result.score,
            "{fen}: SEE pruning changed the score"
        );
        assert_eq!(
            pruned_result.best_move, plain_result.best_move,
            "{fen}: SEE pruning changed the best move"
        );
    }
}

/// A capture that loses material by the swing-off but gives check can still deliver mate, so the
/// quiescence SEE cut must exempt it. Here the one mating move, Qxg7#, is a queen taking a
/// pawn — a swing-off of roughly minus eight pawns — yet a bare quiescence search finds the mate
/// and skips nothing, because the check exemption keeps the capture in the search.
#[test]
fn quiescence_finds_a_mate_delivered_by_a_losing_capture() {
    chess::init::init_globals();

    let position = Position::from_fen("6k1/5ppp/7Q/5N2/8/8/8/6K1 w - - 0 1").unwrap();
    let flag = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(position, &flag, None, &table);

    let value = search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P, 0);

    assert_eq!(value, Some(Score::mate(1)), "quiescence missed Qxg7#");
    assert_eq!(
        search.trace.see_skipped_nodes(),
        0,
        "the checking mate capture was wrongly cut"
    );
}

/// Quiescence must skip captures the swing-off scores as losing: they hand the opponent a
/// favourable recapture and cannot improve the stand-pat value. On a capture-rich middlegame the
/// cut fires several times and collapses the quiescence subtree, and — because a losing capture
/// could never have raised the score anyway — the value it returns is identical to the value with
/// the cut switched off.
#[test]
fn quiescence_skips_losing_captures() {
    chess::init::init_globals();

    let fen = "2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14";
    let position = Position::from_fen(fen).unwrap();
    let flag = AtomicBool::new(false);

    let on_table = Table::new(16);
    let mut on = Search::new(position.clone(), &flag, None, &on_table);
    let on_value = on.quiesce::<Master, Pv>(Score::cp(-2000), Score::cp(2000), 0);

    let off_table = Table::new(16);
    let mut off = Search::new(position, &flag, None, &off_table);
    off.see_pruning_disabled = true;
    let off_value = off.quiesce::<Master, Pv>(Score::cp(-2000), Score::cp(2000), 0);

    assert_eq!(
        on_value, off_value,
        "the SEE cut changed the quiescence value"
    );
    assert!(
        on.trace.see_skipped_nodes() > 0,
        "no losing capture was skipped"
    );
    assert!(
        on.trace.q_nodes_visited() < off.trace.q_nodes_visited(),
        "the cut did not shrink the quiescence tree: {} vs {}",
        on.trace.q_nodes_visited(),
        off.trace.q_nodes_visited(),
    );
}

/// The delta cut is distinct from the losing-capture cut: it discards even a *winning* capture
/// when the piece it wins cannot lift the stand-pat value to alpha. The same non-losing capture
/// (a pawn grab, SEE positive) is searched under a low alpha it can reach, but skipped under a
/// high alpha it cannot — isolating the margin as the cause.
#[test]
fn quiescence_delta_margin_skips_out_of_reach_captures() {
    chess::init::init_globals();

    // White can play exd5, winning an undefended pawn (SEE positive, so the losing-capture cut
    // never applies). Stand pat is level.
    let fen = "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1";
    let position = Position::from_fen(fen).unwrap();
    let flag = AtomicBool::new(false);

    // Under a window the pawn grab can reach, the capture is searched.
    let reachable_table = Table::new(16);
    let mut reachable = Search::new(position.clone(), &flag, None, &reachable_table);
    reachable.quiesce::<Master, Pv>(Score::cp(-500), Score::cp(-400), 0);
    assert_eq!(
        reachable.trace.see_skipped_nodes(),
        0,
        "a reachable capture was pruned by the delta margin"
    );

    // Under a window even the won pawn cannot reach, the delta margin prunes it.
    let hopeless_table = Table::new(16);
    let mut hopeless = Search::new(position, &flag, None, &hopeless_table);
    hopeless.quiesce::<Master, Pv>(Score::cp(500), Score::cp(600), 0);
    assert!(
        hopeless.trace.see_skipped_nodes() > 0,
        "the delta margin did not prune a hopeless capture"
    );
}

/// While the side to move is in check there is no stand pat and every evasion must be searched,
/// so neither quiescence cut may fire — the in-check node routes through `quiesce_evasions`,
/// which has no cut at all. Here the only legal reply is a losing capture, Qxe2, taking a rook
/// that a bishop immediately recaptures; a quiescence search returns the same value whether the
/// cuts are on or off, and that value reflects the forced capture rather than a false checkmate.
#[test]
fn quiescence_cuts_do_not_apply_while_in_check() {
    chess::init::init_globals();

    let fen = "4k3/8/8/1b6/8/7b/4r3/3QK3 w - - 0 1";
    let position = Position::from_fen(fen).unwrap();
    assert!(position.in_check(), "test position must be in check");
    let flag = AtomicBool::new(false);

    let on_table = Table::new(16);
    let mut on = Search::new(position.clone(), &flag, None, &on_table);
    let on_value = on.quiesce::<Master, Pv>(Score::cp(-2000), Score::cp(2000), 0);

    let off_table = Table::new(16);
    let mut off = Search::new(position, &flag, None, &off_table);
    off.see_pruning_disabled = true;
    let off_value = off.quiesce::<Master, Pv>(Score::cp(-2000), Score::cp(2000), 0);

    assert_eq!(
        on_value, off_value,
        "the cuts changed the value of an in-check node"
    );
    assert_ne!(
        on_value,
        Some(Score::mate(0)),
        "the forced evasion was not searched"
    );
}

/// SEE pruning must earn its keep: on a quiet middlegame it visits strictly fewer nodes than the
/// same fixed-depth search with the cuts switched off, and the skip counter confirms the cuts
/// actually fired rather than being dead code behind their guards.
#[test]
fn see_pruning_shrinks_the_search_tree() {
    chess::init::init_globals();

    let position =
        Position::from_fen("r3k2r/pppq1ppp/2np1n2/4p3/2B1P3/2NP1N2/PPP2PPP/R2Q1RK1 w kq - 0 1")
            .unwrap();
    let flag = AtomicBool::new(false);
    let depth = 7;

    let pruned_table = Table::new(16);
    let mut pruned = Search::new(position.clone(), &flag, None, &pruned_table);
    pruned.run::<Master>(depth).unwrap();

    let plain_table = Table::new(16);
    let mut plain = Search::new(position, &flag, None, &plain_table);
    plain.see_pruning_disabled = true;
    plain.run::<Master>(depth).unwrap();

    assert!(
        pruned.trace.see_skipped_nodes() > 0,
        "SEE pruning never fired"
    );
    assert!(
        pruned.trace.all_nodes_visited() < plain.trace.all_nodes_visited(),
        "SEE pruning did not shrink the tree: {} pruned vs {} plain",
        pruned.trace.all_nodes_visited(),
        plain.trace.all_nodes_visited(),
    );
}

/// A search of a position that is already drawn — here by the fifty-move rule — still owes a
/// legal move. Every iteration returns the draw score with no principal variation, because a
/// drawn root scores zero without any move raising alpha, so the deepest completed iteration
/// carries an empty PV. Reporting that iteration would hand back a null move — a `bestmove 0000`
/// forfeit — even though the position has legal moves. This became easy to hit once forward
/// pruning let a dead-drawn endgame race to a very high depth in a sliver of time.
#[test]
fn a_drawn_root_still_reports_a_legal_move() {
    chess::init::init_globals();

    // White to move, fifty-move counter already at the 100-ply draw boundary, but with legal
    // moves available (not stalemate).
    let position = Position::from_fen("6k1/8/6K1/8/8/8/8/6R1 w - - 100 80").unwrap();
    let legal = position.generate::<BasicMoveList, AllGen, Legal>();
    assert!(!legal.is_empty(), "test position must have legal moves");

    let flag = AtomicBool::new(false);
    let tt = Table::new(16);
    let mut search = Search::new(position, &flag, None, &tt);
    let result = search.run::<Master>(8).unwrap();

    let mov = result
        .best_move
        .expect("a drawn but non-terminal root must still return a legal move, not a null move");
    assert!(
        (&legal).into_iter().any(|legal_move| *legal_move == mov),
        "the reported move {mov:?} is not legal in the position",
    );
}

/// The static evaluation reported for a position must be the leaf value the search would use,
/// expressed from the side to move's perspective — a positive score favouring the side to move —
/// so that one position's evaluation can be inspected apart from any search.
#[test]
fn static_eval_reports_the_hand_crafted_leaf_from_the_side_to_moves_view() {
    use crate::eval::Evaluation;

    // The same board with each side to move. Black is up a rook, so the hand-crafted evaluation,
    // which is from White's perspective, is negative for this board regardless of whose turn it is.
    let white_to_move = Position::from_fen("r5k1/8/8/8/8/8/8/6K1 w - - 0 1").unwrap();
    let black_to_move = Position::from_fen("r5k1/8/8/8/8/8/8/6K1 b - - 0 1").unwrap();

    let mut engine = SearchEngine::new(1);
    // Pin the hand-crafted evaluation so the assertion holds whether or not this build embeds a net.
    engine.set_network(None);

    let white_view = engine.static_eval(&white_to_move);
    let black_view = engine.static_eval(&black_to_move);

    // The White-perspective evaluation is a property of the board, not of whose turn it is; the
    // side-to-move report equals it for White and negates it for Black.
    assert_eq!(white_view, white_to_move.static_eval());
    assert_eq!(black_view, -white_to_move.static_eval());
    assert!(
        white_view < 0,
        "with Black up a rook, White's own view is negative"
    );
    assert!(
        black_view > 0,
        "with Black up a rook, Black's own view is positive"
    );
}

/// The accumulator the search maintains across its own `make_move`/`unmake_move` (and null-move)
/// seam is bit-identical to a from-scratch rebuild at every node of a subtree, for every move kind:
/// quiet moves, captures, castling both sides, en passant, promotions with and without capture, and
/// null moves.
///
/// `Accumulator::from_position` is the from-scratch reference; the search folds each move in
/// incrementally instead. Walking whole subtrees through the real search wrappers catches an update
/// that stays self-consistent per move but drifts over a deep line, which a single make/unmake could
/// not. The checks are `assert_eq!` rather than the search's debug-only guard, so they hold in
/// release builds too. Bit-identical accumulators feed the forward pass identical inputs, so this is
/// also what makes the NNUE leaf value — and therefore any fixed-depth search — identical to the
/// earlier from-scratch behaviour.
#[test]
fn search_maintains_the_accumulator_bit_identically_to_a_rebuild() {
    chess::init::init_globals();
    let net = Arc::new(test_network());

    fn walk(search: &mut Search<'_>, net: &Network, depth: u32) {
        assert_eq!(
            *search.accumulator.as_ref().unwrap(),
            Accumulator::from_position(net, &search.pos).into_values(),
            "maintained accumulator disagrees with a rebuild before descending"
        );

        // A null move is legal only out of check. It moves no piece, so the accumulator is carried
        // across unchanged and then restored exactly on unmake.
        if !search.pos.in_check() {
            search.make_null_move();
            assert_eq!(
                *search.accumulator.as_ref().unwrap(),
                Accumulator::from_position(net, &search.pos).into_values(),
                "a null move changed the accumulator"
            );
            search.unmake_null_move();
            assert_eq!(
                *search.accumulator.as_ref().unwrap(),
                Accumulator::from_position(net, &search.pos).into_values(),
                "the accumulator was not restored after a null move"
            );
        }

        if depth == 0 {
            return;
        }

        let moves = search.pos.generate::<BasicMoveList, AllGen, Legal>();
        for mov in &moves {
            // SAFETY: `mov` was just generated as a legal move for the current position.
            unsafe { search.make_move(mov) };
            assert_eq!(
                *search.accumulator.as_ref().unwrap(),
                Accumulator::from_position(net, &search.pos).into_values(),
                "the accumulator diverged after {mov}"
            );
            walk(search, net, depth - 1);
            search.unmake_move();
            assert_eq!(
                *search.accumulator.as_ref().unwrap(),
                Accumulator::from_position(net, &search.pos).into_values(),
                "the accumulator was not restored after unmaking {mov}"
            );
        }
    }

    // Depths kept modest so the from-scratch check at every node stays cheap while still forcing long
    // make/unmake sequences through each position's characteristic move kinds.
    let cases = [
        // The opening: quiet development and the first captures.
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            3,
        ),
        // Kiwipete: castling for both sides, captures of every piece, and pins.
        (
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            3,
        ),
        // A pawn poised for a double push that creates a real en-passant target.
        ("4k3/8/8/8/5p2/8/4P3/4K3 w - - 0 1", 4),
        // Pawns on the seventh for both sides: promotions, including promotion captures.
        ("n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1", 3),
    ];

    for (fen, depth) in cases {
        let pos = Position::from_fen(fen).expect("test FEN is valid");
        let flag = AtomicBool::new(false);
        let tt = Table::new(1);
        let mut search = Search::new(pos, &flag, None, &tt);
        // Selecting the network seeds the accumulator from the root, the path the walk relies on.
        search.set_network(Some(Arc::clone(&net)));
        walk(&mut search, &net, depth);
    }
}

/// A fixed-depth search that scores through the network runs entirely on the incrementally
/// maintained accumulator and returns the same result every time. Together with the per-node
/// equivalence proven by `search_maintains_the_accumulator_bit_identically_to_a_rebuild`, which
/// guarantees the leaf values match a from-scratch rebuild, this exercises the whole integrated
/// path and pins its determinism.
#[test]
fn fixed_depth_network_search_is_deterministic() {
    chess::init::init_globals();
    let net = test_network();

    // Depth is kept shallow: every node rebuilds the accumulator from scratch under the debug-build
    // equivalence guard, so a deeper search would make this test disproportionately slow for the
    // narrow property it pins.
    for fen in [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    ] {
        let pos = Position::from_fen(fen).expect("test FEN is valid");

        let run = || {
            let flag = AtomicBool::new(false);
            let tt = Table::new(1);
            let mut search = Search::new(pos.clone(), &flag, None, &tt);
            search.set_network(Some(Arc::new(net.clone())));
            search
                .run::<Master>(3)
                .expect("a non-terminal position returns a result")
        };

        let first = run();
        let second = run();
        assert_eq!(
            first.score, second.score,
            "network search score is not deterministic on {fen}"
        );
        assert_eq!(
            first.best_move, second.best_move,
            "network search best move is not deterministic on {fen}"
        );
    }
}
