use crate::history::HistoryTable;

use super::eval::Evaluation;
use super::info::{CurrMoveInfo, Info, PvInfo};
use super::killer::KillerTable;
use super::ordering::{Loader, OrderedMoves, ScoredMoveList, Scorer};
use super::pv_table::PVTable;
use super::score::Score;
use super::trace::Tracer;
use super::tt::{Bound, Table};

use core::mono_traits::{All, Captures, Legal, QueenPromotions, Quiets};
use core::mov::Move;
use core::movelist::{BasicMoveList, MoveList};
use core::position::{Player, Position};

use separator::Separatable;

use std::ops::Neg;
use std::sync::atomic::{AtomicBool, Ordering};

pub const INFINITY: i32 = 10_000;

/// Manages the search.
pub struct Search<'engine> {
    /// The internal board position.
    pub(super) pos: Position,
    /// Table for tracking the principal variation of the search.
    pvt: PVTable,
    /// Tracer to track search stats.
    trace: Tracer,
    /// The transposition table.
    tt: &'engine Table,
    /// The killer move table.
    kt: KillerTable,
    /// The history table.
    history: HistoryTable,
    /// Flag to indicate when the search should start unwinding due to user intervention.
    stopping: &'engine AtomicBool,
    search_depth: u8,
}

impl<'engine> Search<'engine> {
    pub fn new(pos: Position, flag: &'engine AtomicBool, tt: &'engine Table) -> Self {
        Self {
            pos,
            tt,
            kt: KillerTable::new(20),
            history: HistoryTable::new(),
            pvt: PVTable::new(8),
            trace: Tracer::new(),
            stopping: flag,
            search_depth: 0,
        }
    }

    pub fn start_search(&mut self, d: u8) -> Score {
        self.trace = Tracer::new();

        // TODO: turn this into a proper use error.
        assert!(d > 0);

        // Some bookeeping and prep.
        // self.pvt = PVTable::new(d);
        self.trace.commence_search();
        self.search_depth = d;

        // let score = self.negamax(d);
        // let score = self.alphabeta_ordered(Score::INF_N, Score::INF_P, d);
        let score = self.iterative_deepening(d);

        self.trace.end_search();
        self.report_pv(d, score);
        self.history.reset();

        score
    }

    fn iterative_deepening(&mut self, depth: u8) -> Score {
        let mut score = Score::INF_N;
        for d in 2..=depth {
            self.pvt = PVTable::new(d);
            self.search_depth = d;
            score = self.alphabeta(Score::INF_N, Score::INF_P, d);
            // score = self.pvs(Score::INF_N, Score::INF_P, d);
            self.report_pv(d, score);
        }

        score
    }

    fn alphabeta(&mut self, mut alpha: Score, mut beta: Score, depth: u8) -> Score {
        self.trace.visit_node();
        let draft = self.search_depth - depth;

        let (tt_entry, tt_mov) = {
            use super::tt::Probe::*;
            match self.tt.probe(&self.pos) {
                Hit(entry) => {
                    let e = entry.read();
                    if e.mov.is_null() {
                        (entry, None)
                    } else {
                        let mov = e.mov.to_move(&self.pos);
                        if self.pos.valid_move(&mov) {
                            self.trace.hash_hit();
                            (entry, Some(mov))
                        } else {
                            self.trace.hash_collision();
                            (entry, None)
                        }
                    }
                }
                Clash(entry) => {
                    self.trace.hash_clash();
                    (entry, None)
                }
                Empty(entry) => (entry, None),
            }
        };

        // Handle leaf node.
        if depth == 0 {
            // let score = self.evaluate();
            let score = self.quiesce(alpha, beta);
            if score == Score::mate(0) {
                self.pvt.pv_leaf_at(0);
            }
            return score;
        }

        // Immediate return, or window adjustment, if the tt move is of a high enough depth.
        let entry = tt_entry.read();
        if entry.depth >= depth && entry.bound() == Bound::Exact {
            return entry.score;
        } else if entry.depth >= depth && entry.bound() == Bound::Lower {
            if entry.score > beta {
                return entry.score; // guaranteed beta-cutoff
            } else if entry.score > alpha {
                alpha = entry.score; // can narrow window
            }
        } else if entry.depth >= depth && entry.bound() == Bound::Upper {
            if entry.score < alpha {
                return entry.score; // guaranteed all-node
            } else if entry.score < beta {
                beta = entry.score; // can narrow window
            }
        }

        let mut max = Score::INF_N;
        let mut best_move = Move::null();

        let mut moves = OrderedMoves::new();
        let mut c = 0;
        let mut did_raise_alpha = false;

        // Iterate the moves.
        while moves.load_next_phase(MoveLoader::from(self, tt_mov, draft)) {
            for mov in &moves {
                c += 1;

                // Start reporting which move we're considering after 2 seconds have elapsed.
                if draft == 0 && self.trace.live_elapsed().as_millis() > 2000 {
                    self.report_curr_move(depth, &mov, c);
                }

                self.pos.make_move(mov);
                let score = self.alphabeta(-beta, -alpha, depth - 1).neg().inc_mate();
                self.pos.unmake_move();

                if self.stopping.load(Ordering::Relaxed) {
                    break;
                }

                if score >= beta {
                    tt_entry.write(&self.pos, score, depth, Bound::Lower, mov);

                    if mov.is_quiet_or_castle() {
                        // This is a killer move, because it's quiet and caused a beta-cutoff.
                        self.kt.store(*mov, draft);

                        // If we had a beta cutoff on a quiet move at least 2 ply from a leaf,
                        // record it in the history table.
                        if depth > 2 {
                            // SAFETY: this is a legal move, so the squares must be valid.
                            unsafe {
                                self.history.inc_unchecked(
                                    mov.orig(),
                                    mov.dest(),
                                    depth as u16 * depth as u16,
                                    self.pos.turn(),
                                );
                            }
                        }
                    }

                    return score;
                }

                if score > max {
                    self.pvt.copy_to(depth, *mov);
                    max = score;
                    best_move = mov.clone();
                    if score > alpha {
                        did_raise_alpha = true;
                        alpha = score;
                    }
                }
            }

            if self.stopping.load(Ordering::Relaxed) {
                return Score::cp(0);
            }
        }

        // If we had no moves.
        if c == 0 {
            self.pvt.pv_leaf_at(depth);

            let score = if self.pos.in_check() {
                Score::mate(0)
            } else {
                Score::cp(0)
            };

            tt_entry.write(&self.pos, score, depth, Bound::Exact, &Move::null());

            return score;
        }

        if did_raise_alpha {
            tt_entry.write(&self.pos, max, depth, Bound::Exact, &best_move);
        } else {
            tt_entry.write(&self.pos, max, depth, Bound::Upper, &Move::null());
        }

        max
    }

    fn negamax(&mut self, depth: u8) -> Score {
        self.trace.visit_node();

        if depth == 0 {
            self.evaluate()
        } else {
            let mut max = Score::INF_N;

            let moves = self.pos.generate::<BasicMoveList, All, Legal>();
            if moves.is_empty() {
                self.pvt.pv_leaf_at(depth);
                return if self.pos.in_check() {
                    Score::mate(0)
                } else {
                    Score::cp(0)
                };
            }

            for mov in &moves {
                self.pos.make_move(mov);
                let score = self.negamax(depth - 1).neg().inc_mate();
                self.pos.unmake_move();

                if score > max {
                    // Because every move should have a score > InfN, this will always get called
                    // at least once.
                    self.pvt.copy_to(depth, *mov);
                    max = score;
                }
            }

            max
        }
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    #[inline(always)]
    fn evaluate(&mut self) -> Score {
        if self.pos.in_checkmate() {
            Score::mate(0)
            // TODO: shouldn't we check for stalemate here too?
        } else {
            Score::cp(self.pos.material_eval() * self.pov())
        }
    }

    /// Returns 1 if the player to move is White, -1 if Black. Useful wherever we are using
    /// evaluation functions in a negamax framework, and have to return the evaluation from the
    /// perspective of the side to move.
    #[inline(always)]
    fn pov(&self) -> i16 {
        match self.pos.turn() {
            Player::WHITE => 1,
            Player::BLACK => -1,
        }
    }

    fn quiesce(&mut self, mut alpha: Score, beta: Score) -> Score {
        self.trace.visit_q_node();

        let stand_pat = self.evaluate();

        if stand_pat >= beta {
            return beta;
        }

        if alpha < stand_pat {
            alpha = stand_pat;
        }

        if self.pos.in_check() {
            // A one move search extension. The main alphabeta function will tell us if we are in
            // checkmate or stalemate, and if not, it will try the possible evasions.
            return self.alphabeta(alpha, beta, 1);
        }

        // TODO: this should look at more than just captures. Checks are important to consider too,
        // but they are harder, as not self-limiting like captures.
        let captures = self.pos.generate::<BasicMoveList, Captures, Legal>();
        let mut score: Score;

        for mov in &captures {
            // Evaluate whether the capture is likely to be favourable with SEE.
            let see_eval = self.see(
                mov.orig(),
                mov.dest(),
                self.pos.piece_at_sq(mov.dest()).type_of(),
                self.pos.piece_at_sq(mov.orig()).type_of(),
            );

            if see_eval < Score::cp(0) {
                self.trace.see_skip_node();
                continue;
            }

            self.pos.make_move(mov);
            score = self.quiesce(-beta, -alpha).neg().inc_mate();
            self.pos.unmake_move();

            if score >= beta {
                return beta;
            }

            if score > alpha {
                alpha = score;
            }
        }

        alpha
    }

    fn report_pv(&self, depth: u8, score: Score) {
        println!(
            "{}",
            Info::Pv(PvInfo {
                depth,
                score,
                time: self.trace.live_elapsed().as_millis() as usize,
                nodes: self.trace.nodes_visited(),
                pv: self
                    .pvt
                    .pv()
                    .map(|m| format!("{}", m))
                    .intersperse(" ".to_string())
                    .collect::<String>(),
                hashfull: self.tt.hashfull(),
                nps: self.trace.live_nps() as u32,
            })
        );
    }

    fn report_curr_move(&self, depth: u8, mov: &Move, num: u8) {
        println!(
            "{}",
            Info::CurrMove(CurrMoveInfo {
                depth,
                currmove: *mov,
                number: num,
            })
        );
    }

    /// Detailed debug info about the search, printed after the end of search in debug mode.
    fn report_telemetry(&self, depth: u8, score: Score) {
        println!(
            "nodes:     {}",
            self.trace.all_nodes_visited().separated_string()
        );
        println!(
            "% q_nodes: {:.2}%",
            self.trace.q_nodes_visited() as f32 / self.trace.all_nodes_visited() as f32 * 100.0
        );
        println!(
            "nps:       {}",
            self.trace
                .nps()
                .expect("`end_search` was called, so this should always work")
                .separated_string()
        );
        println!(
            "see skips: {}",
            self.trace.see_skipped_nodes().separated_string()
        );
        println!(
            "time:      {}ms",
            self.trace
                .elapsed()
                .expect("we called `end_search`")
                .as_millis()
                .separated_string()
        );
        println!(
            "eff. bf:   {}",
            self.trace.eff_branching(depth).separated_string()
        );
        println!("tt stats ----------------");
        println!(
            " size: {}MB, slots: {}",
            self.tt.capacity_mb(),
            self.tt.capacity_entries().separated_string()
        );
        println!(
            " hits:       {:>8} ({:.1}%)",
            self.trace.hash_hits().separated_string(),
            self.trace.hash_hits() as f64 / self.trace.hash_probes() as f64 * 100.
        );
        println!(
            " collisions: {:>8} ({:.1}%)",
            self.trace.hash_collisions().separated_string(),
            self.trace.hash_collisions() as f64 / self.trace.hash_probes() as f64 * 100.
        );
        println!(
            " clashes:    {:>8} ({:.1}%)",
            self.trace.hash_clashes().separated_string(),
            self.trace.hash_clashes() as f64 / self.trace.hash_probes() as f64 * 100.
        );
        println!(" hashfull: {:.2}%", self.tt.hashfull() as f64 / 10.);
        println!("-------------------------");
        println!(
            "pv:        {}",
            self.pvt
                .pv()
                .map(|m| m.to_uci_string())
                .collect::<Vec<String>>()
                .join(" ")
        );
        println!("score:     {:?}", score);
        println!(
            "tt move found at {:.2}% of nodes",
            self.trace.hash_found.avg() * 100_f64
        );
        println!(
            "killers found per node: {:.2}",
            self.trace.killers_per_node.avg() * 2_f64
        );
    }
}

pub struct MoveLoader<'a, 'search> {
    search: &'a mut Search<'search>,
    hash_move: Option<Move>,
    draft: u8,
}

impl<'a, 'engine> MoveLoader<'a, 'engine> {
    /// Create a `MoveLoader` from the passed `Search`.
    #[inline(always)]
    pub fn from(search: &'a mut Search<'engine>, hash_move: Option<Move>, draft: u8) -> Self {
        MoveLoader {
            search,
            hash_move,
            draft,
        }
    }
}

impl<'a, 'search> Loader for MoveLoader<'a, 'search> {
    #[inline]
    fn load_hash(&mut self, movelist: &mut ScoredMoveList) {
        match self.hash_move {
            Some(mv) => {
                self.search.trace.hash_found.push(1);
                movelist.push(mv)
            }
            None => {
                self.search.trace.hash_found.push(0);
            }
        }
    }

    fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
        self.search
            .pos
            .generate_in::<_, QueenPromotions, Legal>(movelist);
    }

    fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Captures, Legal>(movelist);
    }

    fn load_killers(&mut self, movelist: &mut ScoredMoveList) {
        let (km1, km2) = self.search.kt.probe(self.draft, &self.search.pos);
        let mut cnt = 0;

        if km1.is_some() {
            cnt += 1;
            movelist.push(km1.unwrap());
        }
        if km2.is_some() {
            cnt += 1;
            movelist.push(km2.unwrap());
        }
        self.search.trace.killers_per_node.push_many(cnt, 2);
    }

    fn load_quiets(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Quiets, Legal>(movelist);
    }

    fn score_captures(&mut self, captures: Scorer) {
        for (mov, score) in captures {
            if mov.is_capture() {
                *score = self.search.see(
                    mov.orig(),
                    mov.dest(),
                    self.search.pos.piece_at_sq(mov.dest()).type_of(),
                    self.search.pos.piece_at_sq(mov.orig()).type_of(),
                );
            }
        }
    }

    fn score_quiets(&mut self, quiets: Scorer) {
        let turn = self.search.pos.turn();
        for (mov, score) in quiets {
            // SAFETY: these are legal moves, so the squares must be valid.
            unsafe {
                *score = Score::cp(
                    self.search
                        .history
                        .get_unchecked(mov.orig(), mov.dest(), turn) as i16,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suite() -> Vec<(&'static str, u8, Score)> {
        // Test position tuples have the form:
        // (fen, depth, value from perpsective of side to move)

        #[rustfmt::skip]
        {
            vec![
                // Mates
                ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6, Score::mate(5)),
                ("5R2/1p1r2pk/p1n1B2p/2P1q3/2Pp4/P6b/1B1P4/2K3R1 w - - 5 3", 6, Score::mate(5)),
                ("1r6/p5pk/1q1p2pp/3P3P/4Q1P1/3p4/PP6/3KR3 w - - 0 36", 6, Score::mate(5)),
                ("1r4k1/p3p1bp/5P1r/3p2Q1/5R2/3Bq3/P1P2RP1/6K1 b - - 0 33", 6, Score::mate(5)),
                ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 6, Score::mate(5)),
                ("5rk1/rb3ppp/p7/1pn1q3/8/1BP2Q2/PP3PPP/3R1RK1 w - - 7 21", 6, Score::mate(5)),
                ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 8, Score::mate(7)),
                ("6k1/1p2qppp/4p3/8/p2PN3/P5QP/1r4PK/8 w - - 0 40", 6, Score::mate(5)),
                ("2R1bk2/p5pp/5p2/8/3n4/3p1B1P/PP1q1PP1/4R1K1 w - - 0 27", 6, Score::mate(5)),
                ("8/7R/r4pr1/5pkp/1R6/P5P1/5PK1/8 w - - 0 42", 6, Score::mate(5)),
                ("r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25", 8, Score::mate(7)),

                // Winning material
                ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 7, Score::cp(300)),
                ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, Score::cp(100)), 
                ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 3, Score::cp(900)),
                ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6, Score::cp(900)),
                ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 4, Score::cp(400)),
                ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 5, Score::cp(400)),
                ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 6, Score::cp(0)),

                // Pawn race
                // ("8/6pk/8/8/8/8/P7/K7 w - - 0 1", 22, Score::cp(800)),
            ]
        }
    }

    /// A regression test to ensure that our search routine produces the expected results for a
    /// range of positions.
    #[test]
    fn gives_correct_answers() {
        core::init::init_globals();

        let suite = suite();

        for (fen, depth, score) in suite {
            let pos = Position::from_fen(fen).unwrap();
            let flag = AtomicBool::new(false);
            let mut search = Search::new(pos, &flag);
            let s = search.start_search(depth);

            assert_eq!(s, score);
        }
    }

    /// Ensure that alphabeta search gives identical result to negamax.
    #[test]
    #[ignore]
    fn ab_equals_negamax() {
        core::init::init_globals();

        let suite = #[rustfmt::skip]
        {
            vec![
                ("2r2k2/pb1q1pp1/1p1b1nB1/3p4/3Nr3/2P1P3/PPQB1PPP/R3K2R w KQ - 3 23", 5, Score::cp(600)),
                ("1n1r1r1k/pp2Ppbp/6p1/4p3/PP2R3/5N1P/6P1/1RBQ2K1 b - - 0 25", 5, Score::cp(100)),
                ("r3k2r/ppb2pp1/2pp3p/P4N2/1PP1n2q/7P/2PB1PP1/R2QR1K1 b kq - 3 17", 3, Score::cp(600)),
                ("r3k2r/1bqp1ppp/p3pn2/1p2n3/1b2P3/2N2B2/PPPBNPPP/R2QR1K1 w kq - 6 12", 5, Score::cp(100)),
            ]
        };

        for (fen, depth, score) in suite {
            let pos = Position::from_fen(fen).unwrap();
            let flag = AtomicBool::new(false);
            let mut search = Search::new(pos, &flag);
            let s_negamax = search.negamax(depth);
            let s_alphabeta = search.alphabeta(Score::INF_N, Score::INF_P, depth);

            assert_eq!(s_negamax, s_alphabeta);
            assert_eq!(score, s_negamax);
        }
    }
}
