use super::eval::Evaluation;
use super::ordering::OrderedMoves;
use super::pv_table::PVTable;
use super::score::Score;
use super::time::TimingMode;
use super::trace::Tracer;

use core::mono_traits::LegalType;
use core::movegen::MoveGen;
use core::movelist::{BasicMoveList, MoveStack, OverflowingMoveList};
use core::position::{Player, Position};

use separator::Separatable;

use std::ops::Neg;

pub const INFINITY: i32 = 10_000;

/// Manages the search.
pub struct Search {
    /// The internal board position.
    pub(super) pos: Position,
    /// Table for tracking the principal variation of the search.
    pvt: PVTable,
    /// Tracer to track search stats.
    trace: Tracer,
    movestack: MoveStack,
}

impl Search {
    pub fn new(pos: Position) -> Self {
        Self {
            pos,
            pvt: PVTable::new(8),
            trace: Tracer::new(),
            movestack: MoveStack::new(),
        }
    }

    pub fn start_search(mut self, tm: TimingMode) -> (Score, Position) {
        match tm {
            TimingMode::Timed(_) => todo!(),
            TimingMode::MoveTime(_) => todo!(),
            TimingMode::Depth(d) => {
                // Some bookeeping and prep.
                self.pvt = PVTable::new(d);
                self.trace.commence_search();

                // let score = self.negamax(d);
                let score = self.alphabeta(Score::INF_N, Score::INF_P, d);

                self.trace.end_search();

                println!(
                    "nodes:     {}",
                    self.trace.all_nodes_visited().separated_string()
                );
                println!(
                    "% q_nodes: {:.2}%",
                    self.trace.q_nodes_visited() as f32 / self.trace.all_nodes_visited() as f32
                        * 100.0
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
                    self.trace.eff_branching(d).separated_string()
                );
                println!(
                    "pv:        {}",
                    self.pvt
                        .pv()
                        .map(|m| m.to_uci_string())
                        .collect::<Vec<String>>()
                        .join(" ")
                );
                // (self.pos, score)
                println!("score:     {:?}", score);
                (score, self.pos)
            }
            TimingMode::Infinite => todo!(),
        }
    }

    fn alphabeta<'search>(&'search mut self, mut alpha: Score, beta: Score, depth: u8) -> Score {
        self.trace.visit_node();

        if depth == 0 {
            self.quiesce(alpha, beta)
            // self.evaluate()
        } else {
            let mut max = Score::INF_N;

            let moves = self.pos.generate_moves::<BasicMoveList>();
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
                let score = self.alphabeta(-beta, -alpha, depth - 1).neg().inc_mate();
                self.pos.unmake_move();

                if score >= beta {
                    return score;
                }

                if score > max {
                    self.pvt.copy_to(depth, *mov);
                    max = score;
                    if score > alpha {
                        alpha = score;
                    }
                }
            }

            max
        }
    }

    fn negamax(&mut self, depth: u8) -> Score {
        self.trace.visit_node();

        if depth == 0 {
            self.evaluate()
        } else {
            let mut max = Score::INF_N;

            let moves = self.pos.generate_moves::<BasicMoveList>();
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
    fn pov(&self) -> i32 {
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

        // TODO: this should look at more than just captures. Checks are important to consider too,
        // but they are harder, as not self-limiting like captures.
        let captures = self.pos.generate_captures();
        let mut score: Score;

        if captures.is_empty() {
            if self.pos.in_checkmate() {
                return Score::mate(0);
            }
            // TODO: we need to deal with stalemate here. If we don't, we might be getting wrong
            // results?
        }

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
                ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 7, Score::mate(7)),
                ("6k1/1p2qppp/4p3/8/p2PN3/P5QP/1r4PK/8 w - - 0 40", 5, Score::mate(5)),
                ("2R1bk2/p5pp/5p2/8/3n4/3p1B1P/PP1q1PP1/4R1K1 w - - 0 27", 5, Score::mate(5)),
                ("8/7R/r4pr1/5pkp/1R6/P5P1/5PK1/8 w - - 0 42", 5, Score::mate(5)),
                ("r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25", 7, Score::mate(7)),

                // Winning material
                ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 7, Score::cp(300)),
                ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, Score::cp(100)), 
                ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 3, Score::cp(900)),
                ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 5, Score::cp(800)),
                ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 3, Score::cp(400)),
                ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 5, Score::cp(600)),
                ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 5, Score::cp(0)),

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
            let search = Search::new(pos);
            let (s, _) = search.start_search(TimingMode::Depth(depth));

            assert_eq!(s, score);
        }
    }

    /// Ensure that alphabeta search gives identical result to negamax.
    #[test]
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
            let mut search = Search::new(pos);
            let s_negamax = search.negamax(depth);
            let s_alphabeta = search.alphabeta(Score::INF_N, Score::INF_P, depth);

            assert_eq!(s_negamax, s_alphabeta);
            assert_eq!(score, s_negamax);
        }
    }
}
