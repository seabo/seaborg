use crate::eval::material_eval;
use crate::mov::Move;
use crate::position::Position;
use crate::tables::Table;
use separator::Separatable;
use std::cmp::{max, min};

#[derive(Clone)]
pub enum NodeType {
    Exact,
    UpperBound,
    LowerBound,
}

#[derive(Clone)]
pub struct TTData {
    depth: u8,
    node_type: NodeType,
    score: i32,
    best_move: Move,
}

pub struct Negamax<'a> {
    pos: &'a mut Position,
    tt: Table<TTData>,
    visited: usize,
}

impl<'a> Negamax<'a> {
    pub fn new(pos: &'a mut Position) -> Self {
        let tt: Table<TTData> = Table::with_capacity(27);
        Negamax {
            pos,
            tt,
            visited: 0,
        }
    }

    pub fn display_trace(&self) {
        println!("Visited {} nodes", self.visited.separated_string());
        self.tt.display_trace();
    }

    pub fn get_best_move(&mut self) -> Option<Move> {
        match self.tt.get(self.pos) {
            Some(data) => Some(data.best_move),
            None => None,
        }
    }

    pub fn recover_pv(&mut self) -> Vec<Move> {
        let mut pv: Vec<Move> = Vec::new();
        let mut length = 0;
        while let Some(mov) = self.get_best_move() {
            pv.push(mov);
            self.pos.make_move(mov);
            length += 1;
        }

        while length > 1 {
            length -= 1;
            self.pos.unmake_move();
        }

        pv
    }

    pub fn iterative_deepening(&mut self, target_depth: u8) -> i32 {
        for i in 1..target_depth + 1 {
            println!("searching depth {}", i);
            self.negamax(i, -10_000, 10_000);
        }

        match self.tt.get(self.pos) {
            Some(data) => data.score,
            None => unreachable!(),
        }
    }

    pub fn negamax(&mut self, depth: u8, mut alpha: i32, mut beta: i32) -> i32 {
        let is_white = self.pos.turn().is_white();
        let alpha_orig = alpha;
        self.visited += 1;

        if self.pos.in_checkmate() {
            return -10_000;
        }

        match self.tt.get(self.pos) {
            Some(data) => {
                if data.depth >= depth {
                    match data.node_type {
                        NodeType::Exact => return data.score,
                        NodeType::LowerBound => alpha = max(alpha, data.score),
                        NodeType::UpperBound => beta = min(beta, data.score),
                    }
                }
            }
            None => {}
        };

        if depth == 0 {
            return material_eval(self.pos) * if is_white { 1 } else { -1 };
        }

        let moves = self.pos.generate_moves();

        if moves.is_empty() {
            if self.pos.in_check() {
                return -10_000;
            } else {
                return 0;
            }
        }

        let mut val = -10_000;
        let mut best_move: Move = moves[0];

        for mov in &moves {
            self.pos.make_move(*mov);
            let score = -self.negamax(depth - 1, -beta, -alpha);
            if score > val {
                val = score;
                best_move = mov.clone();
            }
            self.pos.unmake_move();
            alpha = max(alpha, val);

            if val >= beta {
                break;
            }
        }

        let node_type = if val <= alpha_orig {
            NodeType::UpperBound
        } else if val >= beta {
            NodeType::LowerBound
        } else {
            NodeType::Exact
        };

        let tt_entry = TTData {
            depth,
            node_type,
            score: val,
            best_move,
        };

        self.tt.insert(self.pos, tt_entry);
        return val;
    }
}
