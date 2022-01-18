use crate::eval::material_eval;
use crate::tables::Table;
use core::position::Position;
use std::cmp::{max, min};

#[derive(Clone)]
pub struct TTData {
    depth: u8,
    score: i32,
}

pub struct ABSearcher<'a> {
    pos: &'a mut Position,
    tt: Table<TTData>,
}

impl<'a> ABSearcher<'a> {
    /// Create a new `Searcher` for the given `Position`.
    pub fn new(pos: &'a mut Position) -> Self {
        ABSearcher {
            pos,
            tt: Table::with_capacity(27),
        }
    }

    pub fn display_trace(&self) {
        self.tt.display_trace();
    }

    pub fn alphabeta(
        &mut self,
        // pos: &mut Position,
        depth: u8,
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

        match self.tt.get(self.pos) {
            Some(data) => {
                if data.depth >= depth {
                    return data.score;
                } else {
                    if is_white {
                        let mut val = -10000;
                        let moves = self.pos.generate_moves();
                        for mov in &moves {
                            self.pos.make_move(*mov);
                            val = max(val, self.alphabeta(depth - 1, alpha, beta, false));
                            self.pos.unmake_move();
                            alpha = max(alpha, val);

                            self.tt.insert(self.pos, TTData { depth, score: val });

                            if val >= beta {
                                break;
                            }
                        }
                        return val;
                    } else {
                        let mut val = 10000;
                        let moves = self.pos.generate_moves();
                        for mov in &moves {
                            self.pos.make_move(*mov);
                            val = min(val, self.alphabeta(depth - 1, alpha, beta, true));
                            self.pos.unmake_move();
                            beta = min(beta, val);

                            self.tt.insert(self.pos, TTData { depth, score: val });
                            if val <= alpha {
                                break;
                            }
                        }
                        return val;
                    }
                }
            }
            None => {
                if is_white {
                    let mut val = -10000;
                    let moves = self.pos.generate_moves();
                    for mov in &moves {
                        self.pos.make_move(*mov);
                        val = max(val, self.alphabeta(depth - 1, alpha, beta, false));
                        self.pos.unmake_move();
                        alpha = max(alpha, val);

                        self.tt.insert(self.pos, TTData { depth, score: val });

                        if val >= beta {
                            break;
                        }
                    }
                    return val;
                } else {
                    let mut val = 10000;
                    let moves = self.pos.generate_moves();
                    for mov in &moves {
                        self.pos.make_move(*mov);
                        val = min(val, self.alphabeta(depth - 1, alpha, beta, true));
                        self.pos.unmake_move();
                        beta = min(beta, val);

                        self.tt.insert(self.pos, TTData { depth, score: val });
                        if val <= alpha {
                            break;
                        }
                    }
                    return val;
                }
            }
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
