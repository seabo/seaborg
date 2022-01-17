use crate::eval::material_eval;
use crate::mov::Move;
use crate::position::Position;
use crate::tables::Table;
use separator::Separatable;
use std::cmp::{max, min};

#[derive(Clone, Debug)]
pub enum NodeType {
    Exact,
    UpperBound,
    LowerBound,
}

#[derive(Clone, Debug)]
pub struct TTData {
    depth: u8,
    node_type: NodeType,
    score: i32,
    best_move: Move,
}

pub struct PVSearch {
    pos: Position,
    tt: Table<TTData>,
    visited: usize,
    moves_considered: usize,
    moves_visited: usize,
}

impl PVSearch {
    pub fn new(pos: Position) -> Self {
        let tt = Table::with_capacity(27);
        PVSearch {
            pos,
            tt,
            visited: 0,
            moves_considered: 0,
            moves_visited: 0,
        }
    }

    pub fn display_trace(&self) {
        println!("Visited {} nodes", self.visited.separated_string());
        println!(
            "Moves considered: {}",
            self.moves_considered.separated_string()
        );
        println!("Moves visited: {}", self.moves_visited.separated_string());
        println!(
            "Pruning factor: {:.4}%",
            ((1 as f32 - self.moves_visited as f32 / self.moves_considered as f32) * 100 as f32)
                .separated_string()
        );
        self.tt.display_trace();
    }

    pub fn get_best_move(&mut self) -> Option<Move> {
        match self.tt.get(&self.pos) {
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

        while length > 0 {
            length -= 1;
            self.pos.unmake_move();
        }

        pv
    }

    pub fn iterative_deepening(&mut self, target_depth: u8) -> i32 {
        for i in 0..target_depth + 1 {
            println!("searching depth {}", i);
            self.pv_search(i, -10_000, 10_000);
        }

        // The TT should always have an entry here, so the unwrap never fails
        self.tt.get(&self.pos).unwrap().score
    }

    pub fn pv_search(&mut self, depth: u8, mut alpha: i32, mut beta: i32) -> i32 {
        let is_white = self.pos.turn().is_white();
        let alpha_orig = alpha;
        self.visited += 1;

        if self.pos.in_checkmate() {
            return -10_000;
        }

        if let Some(data) = self.tt.get(&self.pos) {
            if data.depth >= depth {
                match data.node_type {
                    NodeType::Exact => return data.score,
                    NodeType::LowerBound => alpha = max(alpha, data.score),
                    NodeType::UpperBound => beta = min(beta, data.score),
                }
            }
        };

        if depth == 0 {
            return material_eval(&self.pos) * if is_white { 1 } else { -1 };
        }

        let moves = self.pos.generate_moves();
        if moves.is_empty() {
            if self.pos.in_check() {
                return -10_000;
            } else {
                return 0;
            }
        }
        let mut search_pv = true;
        let mut val = -10_000;
        let mut best_move: Move = moves[0];
        self.moves_considered += moves.len();

        for mov in &moves {
            self.moves_visited += 1;
            self.pos.make_move(*mov);
            let mut score: i32;
            if search_pv {
                score = -self.pv_search(depth - 1, -beta, -alpha);
            } else {
                score = -self.pv_search(depth - 1, -alpha - 100, -alpha);
                if score > alpha {
                    // re-search
                    score = -self.pv_search(depth - 1, -beta, -alpha);
                }
            }
            self.pos.unmake_move();
            if score > val {
                val = score;
                search_pv = false;
                best_move = mov.clone();
            }
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

        self.tt.insert(&self.pos, tt_entry);
        return val;
    }
}
