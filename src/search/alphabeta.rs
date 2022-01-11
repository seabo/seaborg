use crate::eval::material_eval;
use crate::position::Position;
use crate::tables::TranspoTable;
use std::cmp::{max, min};

pub struct ABSearcher<'a> {
    pos: &'a mut Position,
    tt: TranspoTable<u8>,
}

impl<'a> ABSearcher<'a> {
    /// Create a new `Searcher` for the given `Position`.
    pub fn new(pos: &'a mut Position) -> Self {
        ABSearcher {
            pos,
            tt: TranspoTable::with_capacity(27),
        }
    }

    pub fn display_trace(&self) {
        self.tt.display_trace();
    }

    pub fn alphabeta(
        &mut self,
        // pos: &mut Position,
        depth: usize,
        mut alpha: i32,
        mut beta: i32,
        is_white: bool,
    ) -> i32 {
        if depth == 0 {
            if self.pos.in_checkmate() {
                return if is_white { -10000 } else { 10000 };
            } else {
                return material_eval(self.pos);
            }
        }
        if is_white {
            let mut val = -10000;
            let moves = self.pos.generate_moves();
            for mov in moves {
                self.pos.make_move(mov);
                // Check TT for a transpo here
                self.tt.insert(self.pos, 0);
                val = max(val, self.alphabeta(depth - 1, alpha, beta, false));
                self.pos.unmake_move();
                alpha = max(alpha, val);
                if val >= beta {
                    break;
                }
            }
            return val;
        } else {
            let mut val = 10000;
            let moves = self.pos.generate_moves();
            for mov in moves {
                self.pos.make_move(mov);
                // Check TT for a transpo here
                self.tt.insert(self.pos, 0);
                val = min(val, self.alphabeta(depth - 1, alpha, beta, true));
                self.pos.unmake_move();
                beta = min(beta, val);
                if val <= alpha {
                    break;
                }
            }
            return val;
        }
    }
}

// pub fn iterative_deepening(pos: &mut Position, target_depth: usize) -> i32 {
//     for i in 1..target_depth {
//         let val = alphabeta(pos, i, -10000, 10000, pos.turn().is_white());
//         println!("Depth: {}, val: {}", i, val);
//     }
//     0
// }
