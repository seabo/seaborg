use super::eval::Evaluation;
use super::time::TimingMode;

use core::position::{Player, Position};

pub const INFINITY: i32 = 10_000;

/// Manages the search.
pub struct Search {
    /// The internal board position.
    pos: Position,
}

impl Search {
    pub fn new(pos: Position) -> Self {
        Self { pos }
    }

    pub fn start_search(mut self, tm: TimingMode) -> (Position, i32) {
        println!("timing mode: {:?}", tm);

        match tm {
            TimingMode::Timed(_) => todo!(),
            TimingMode::MoveTime(_) => todo!(),
            TimingMode::Depth(d) => {
                let score = self.negamax(d);
                println!("result: {}", score);
                (self.pos, score)
            }
            TimingMode::Infinite => todo!(),
        }
    }

    fn negamax(&mut self, depth: u8) -> i32 {
        // Basic negamax search

        if depth == 0 {
            let e = self.pos.material_eval()
                * if self.pos.turn() == Player::WHITE {
                    1
                } else {
                    -1
                };

            e
        } else {
            let mut max = -INFINITY;

            let moves = self.pos.generate_moves();
            if moves.len() == 0 {
                return if self.pos.in_check() { -INFINITY } else { 0 };
            }

            for mov in &moves {
                self.pos.make_move(*mov);
                let score = -self.negamax(depth - 1);
                if score > max {
                    max = score;
                }
                self.pos.unmake_move();
            }

            max
        }
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

                // Winning material
                ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 6, 300),
                ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, 100),
                ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 4, 900),
                ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6, 800),
                ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 4, 400),
                ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 6, 500),
                ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 6, 0),
            ]
        };

        for (p, d, v) in suite {
            let (_, res) =
                Search::new(Position::from_fen(p).unwrap()).start_search(TimingMode::Depth(d));

            assert_eq!(res, v);
        }
    }
}
