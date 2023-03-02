use super::eval::Evaluation;
use super::pv_table::PVTable;
use super::score::Score;
use super::time::TimingMode;

use core::position::{Player, Position};

pub const INFINITY: i32 = 10_000;

/// Manages the search.
pub struct Search {
    /// The internal board position.
    pos: Position,
    /// Table for tracking the principal variation of the search.
    pvt: PVTable,
}

impl Search {
    pub fn new(pos: Position) -> Self {
        Self {
            pos,
            pvt: PVTable::new(8),
        }
    }

    pub fn start_search(mut self, tm: TimingMode) -> (Position, i32) {
        match tm {
            TimingMode::Timed(_) => todo!(),
            TimingMode::MoveTime(_) => todo!(),
            TimingMode::Depth(d) => {
                self.pvt = PVTable::new(d);
                // let score = self.alphabeta(-INFINITY, INFINITY, d);
                // let score = self.negamax(d);
                let score = self.alphabeta_with_mate(Score::InfN, Score::Value(10_000), d);
                println!(
                    "pv: {}",
                    self.pvt
                        .pv()
                        .map(|m| m.to_uci_string())
                        .collect::<Vec<String>>()
                        .join(" ")
                );
                // (self.pos, score)
                println!("{:?}", score);
                (self.pos, 0)
            }
            TimingMode::Infinite => todo!(),
        }
    }

    fn alphabeta_with_mate(&mut self, mut alpha: Score, beta: Score, depth: u8) -> Score {
        if depth == 0 {
            // self.quiesce(alpha, beta)
            self.evaluate_score()
        } else {
            let mut max = Score::InfN;

            let moves = self.pos.generate_moves();
            if moves.is_empty() {
                self.pvt.end_of_line_at(depth);
                return if self.pos.in_check() {
                    Score::Mate(0)
                } else {
                    Score::Value(0)
                };
            }

            for mov in &moves {
                self.pos.make_move(*mov);
                let score = (-self.alphabeta_with_mate(-beta, -alpha, depth - 1)).inc_mate();
                self.pos.unmake_move();

                if score >= beta {
                    self.pvt.update_at(depth, *mov);
                    return score;
                }

                if score > max {
                    max = score;
                    if score > alpha {
                        self.pvt.update_at(depth, *mov);
                        alpha = score;
                    }
                }
                if depth == 5 {
                    println!(
                        "trying move: {}; score: {:?}, max -> {:?}",
                        *mov, score, max
                    );
                }
            }
            max
        }
    }

    fn alphabeta(&mut self, mut alpha: i32, beta: i32, depth: u8) -> i32 {
        if depth == 0 {
            // self.quiesce(alpha, beta)
            self.evaluate()
        } else {
            let mut max = -INFINITY;

            let moves = self.pos.generate_moves();
            if moves.is_empty() {
                self.pvt.end_of_line_at(depth);
                return if self.pos.in_check() { -INFINITY } else { 0 };
            }

            for mov in &moves {
                self.pos.make_move(*mov);
                let score = -self.alphabeta(-beta, -alpha, depth - 1);
                self.pos.unmake_move();

                if score >= beta {
                    self.pvt.update_at(depth, *mov);
                    return score; // fail-soft beta-cutoff
                }

                if score > max {
                    max = score;
                    if score > alpha {
                        self.pvt.update_at(depth, *mov);
                        alpha = score;
                    }
                }
            }

            // If max is still -infinity, then this position is somewhere in a forced mate
            // sub-tree. Because we never raised max, we haven't populated any of the PVT.
            // TODO: the best way to do this would be to have the shortest mate written into the PV
            // table, but that's a bit non-trivial.
            if max == -INFINITY {
                self.pvt
                    .update_at(depth, *moves.first().unwrap_or(&core::mov::Move::null()));
            }

            max
        }
    }

    fn negamax(&mut self, depth: u8) -> Score {
        if depth == 0 {
            self.evaluate_score()
        } else {
            let mut max = Score::InfN;

            let moves = self.pos.generate_moves();
            if moves.is_empty() {
                self.pvt.end_of_line_at(depth);
                return if self.pos.in_check() {
                    Score::Mate(0)
                } else {
                    Score::Value(0)
                };
            }

            for mov in &moves {
                self.pos.make_move(*mov);
                let score = (-self.negamax(depth - 1)).inc_mate();
                self.pos.unmake_move();

                if score > max {
                    // Because every move should have a score > InfN, this will always get called
                    // at least once.
                    self.pvt.update_at(depth, *mov);
                    max = score;
                }
            }

            max
        }
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    #[inline(always)]
    fn evaluate(&mut self) -> i32 {
        if self.pos.in_checkmate() {
            -INFINITY
            // TODO: shouldn't we check for stalemate here too?
        } else {
            self.pos.material_eval() * self.pov()
        }
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    #[inline(always)]
    fn evaluate_score(&mut self) -> Score {
        if self.pos.in_checkmate() {
            Score::Mate(0)
            // TODO: shouldn't we check for stalemate here too?
        } else {
            Score::Value(self.pos.material_eval() * self.pov())
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

    fn quiesce(&mut self, mut alpha: i32, beta: i32) -> i32 {
        let stand_pat = self.evaluate();

        if stand_pat >= beta {
            return beta;
        }

        if alpha < stand_pat {
            alpha = stand_pat;
        }

        let captures = self.pos.generate_captures();
        let mut score: i32;

        if captures.is_empty() {
            // If we have no captures to look at, we might actually be in checkmate. Return
            // -infinity if so.
            return -INFINITY;
        }

        for mov in &captures {
            self.pos.make_move(*mov);
            score = -self.quiesce(-beta, -alpha);
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

    #[test]
    fn tactics() {
        core::init::init_globals();

        // Test position tuples have the form:
        // (fen, depth, value from perpsective of side to move)

        #[rustfmt::skip]
        let suite = {
            [
                // Mates
                ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6, INFINITY),
                ("5R2/1p1r2pk/p1n1B2p/2P1q3/2Pp4/P6b/1B1P4/2K3R1 w - - 5 3", 6, INFINITY),
                ("1r6/p5pk/1q1p2pp/3P3P/4Q1P1/3p4/PP6/3KR3 w - - 0 36", 6, INFINITY),
                ("1r4k1/p3p1bp/5P1r/3p2Q1/5R2/3Bq3/P1P2RP1/6K1 b - - 0 33", 6, INFINITY),
                ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 6, INFINITY),
                ("5rk1/rb3ppp/p7/1pn1q3/8/1BP2Q2/PP3PPP/3R1RK1 w - - 7 21", 6, INFINITY),
                // 6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34
                // ("6k1/1p2qppp/4p3/8/p2PN3/P5QP/1r4PK/8 w - - 0 40", 5, Score::Mate(5)),
                // ("2R1bk2/p5pp/5p2/8/3n4/3p1B1P/PP1q1PP1/4R1K1 w - - 0 27", 5, Score::Mate(5)),
                // ("8/7R/r4pr1/5pkp/1R6/P5P1/5PK1/8 w - - 0 42", 5, Score::Mate(5)),

                // Winning material
                ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 6, 300),
                ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, 100),
                ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 4, 900),
                ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6, 800),
                ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 4, 400),
                ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 6, 500),
                ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 6, 0),

                // Pawn race
                ("8/6pk/8/8/8/8/P7/K7 w - - 0 1", 22, 800),
            ]
        };

        for (p, d, v) in suite {
            let (_, res) =
                Search::new(Position::from_fen(p).unwrap()).start_search(TimingMode::Depth(d));

            assert_eq!(res, v);
        }
    }
}
