//! The `Search` module used by `Engine`.

// Note: using the TT can cause 'instability' (in the sense that we can end up
// with a different result from not using the TT) because:
// - if you get a TT hit from a previous search at a _higher_ depth then you
//   definitely want to use this information, but it won't give precisely the same
//   answer as if you had run a plain search with no TT (the answer should actually
//   be better)
//
// As a test of correctness, we should follow this
// [advice](https://webdocs.cs.ualberta.ca/~jonathan/PREVIOUS/Courses/657/A1.pdf):
//
//   If you initially restrict a TT lookup to be valid only if the table depth exactly
//   matches the depth that you need, then the TT will not change the result of a
//   fixed-depth alpha-beta search. It should, however, reduce the number of nodes
//   searched. Verify that this is working correctly.
//
//   Add in iterative deepening and move ordering. If you do this right, it should not
//   change the final result of the search but, again, it should reduce the number of
//   nodes searched.
//
//   Only when you are sure all the above is 100% working should you move on to more
//   search enhancements and a better evaluation function.
//
// TODO: set up some automated test suite which assesses whether we are getting precisely
// accurate equivalent results as plain alpha-beta search, when restricting the TT like
// this.

use crate::eval::material_eval;
use crate::tables::Table;
use core::mov::Move;
use core::movelist::MoveList;
use core::position::Position;
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

pub struct Search {
    pos: Position,
    tt: Table<TTData>,
    visited: usize,
    moves_considered: usize,
    moves_visited: usize,
}

impl Search {
    pub fn new(pos: Position) -> Self {
        Search {
            pos,
            tt: Table::with_capacity(27),
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

        let mut tt_move: Option<Move> = None;

        if let Some(data) = self.tt.get(&self.pos) {
            tt_move = Some(data.best_move);

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

        let mut best_move: Move = moves[0];
        self.moves_considered += moves.len();
        let mut search_pv = true;
        let mut val = -10_000;
        let ordered_moves = OrderedMoveList::new(moves, tt_move);
        for mov in ordered_moves {
            self.moves_visited += 1;
            self.pos.make_move(mov);
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

/// A wrapper around a `MoveList`.
///
/// Implements `Iterator` and uses a 'selection sort' style algorithm
/// to return `Move`s in a priority ordering.
///
/// An `OrderedMoveList` consumes the underlying `MoveList`, so it won't
/// be available after the iteration.
pub struct OrderedMoveList {
    /// The underlying `MoveList`. This gets consumed by the `OrderedMoveList`
    /// and won't be available after the iteration.
    move_list: MoveList,
    /// A copy of the move currently in the transposition table for this position
    tt_move: Move,
    /// Tracks how many `Move`s have so far been yielded by the iteration.
    /// When this reaches `MoveList.len` then we can halt the iteration by
    /// returning `None`.
    yielded: usize,
    /// Tracks whether we have yielded the transposition table move yet
    yielded_tt_move: bool,
    /// Tracks whether we have yielded every capture yet
    yielded_all_captures: bool,
}

impl OrderedMoveList {
    pub fn new(move_list: MoveList, tt_move: Option<Move>) -> Self {
        if let Some(tt_move) = tt_move {
            Self {
                move_list,
                tt_move,
                yielded: 0,
                yielded_tt_move: false,
                yielded_all_captures: false,
            }
        } else {
            Self {
                move_list,
                tt_move: Move::null(),
                yielded: 0,
                yielded_tt_move: true,
                yielded_all_captures: false,
            }
        }
    }
}

impl Iterator for OrderedMoveList {
    type Item = Move;
    fn next(&mut self) -> Option<Self::Item> {
        if self.yielded == self.move_list.len() {
            None
        } else {
            // 1. Do we need to yield the TT move?
            if !self.yielded_tt_move {
                self.yielded += 1;
                self.yielded_tt_move = true;
                return Some(self.tt_move);
            }
            // 2. Do we need to yield captures
            if !self.yielded_all_captures {
                // Yes - scan for the first capture
                for i in 0..self.move_list.len() {
                    let mov = unsafe { self.move_list.get_unchecked_mut(i) };
                    if mov.is_capture() {
                        self.yielded += 1;
                        let returned_move = mov.clone();
                        // set that entry to a null move and return it
                        *mov = Move::null();
                        return Some(returned_move);
                    }
                }
                // If we get here, then nothing was a capture
                self.yielded_all_captures = true;
            }
            // More blocks of moves according to some predicate (like `is_capture()`) would go here and follow the pattern of 2.
            // 3. Yield any remaining moves
            for i in 0..self.move_list.len() {
                let mov = unsafe { self.move_list.get_unchecked_mut(i) };
                if !mov.is_null() {
                    self.yielded += 1;
                    let returned_move = mov.clone();
                    // set that entry to a null move and return it
                    *mov = Move::null();
                    return Some(returned_move);
                }
            }
            // if we get all the way here, then we didn't find any moves at all, so return `None`
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::init::init_globals;
    use core::position::Position;
    #[test]
    fn orders_moves() {
        init_globals();

        let pos = Position::from_fen("4b3/4B1bq/p2Q2pp/4pp2/8/8/p7/k1K5 w - - 0 1").unwrap();
        let move_list = pos.generate_moves();
        let tt_move = move_list[4].clone();
        let mut ordered_move_list = OrderedMoveList::new(move_list, Some(tt_move));

        assert_eq!(ordered_move_list.next().unwrap(), tt_move);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), false);
    }
}
