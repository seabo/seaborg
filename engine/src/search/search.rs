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

use super::ordering::OrderedMoveList;
use super::params::Params;

use crate::engine::Report;
use crate::eval::material_eval;
use crate::sess::Message;
use crate::tables::Table;
use crate::time::TimeControl;

use core::mov::Move;
use core::position::Position;

use crossbeam_channel::Sender;
use separator::Separatable;

use std::cmp::{max, min};
use std::sync::{Arc, RwLock};

/// Represents the search mode specified in a `go` command. The `go` keyword
/// can either be followed by `infinite` which means the position should be
/// searched indefinitely in 'analysis mode', or it can be followed by a
/// string like `wtime 10000 btime 10000 winc 1000 binc 1000 movestogo 5`
/// which represents the clock situation in a timed game.
#[derive(Copy, Clone, Debug)]
pub enum SearchMode {
    Infinite,
    Timed(TimeControl),
}

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
    sender: Option<Sender<Message>>,
    halt: Arc<RwLock<bool>>,
    visited: usize,
    moves_considered: usize,
    moves_visited: usize,
}

impl Search {
    pub fn new(
        mut params: Params,
        sender: Option<Sender<Message>>,
        halt: Arc<RwLock<bool>>,
    ) -> Self {
        let pos = params.take_pos();
        println!("{}", pos);
        let tt = Table::with_capacity(params.tt_cap);

        Search {
            pos,
            tt,
            sender,
            halt,
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
            if self.is_halted() {
                break;
            }

            println!("searching depth {}", i);
            self.pv_search(i, -10_000, 10_000);
        }

        // The TT should always have an entry here, so the unwrap never fails.
        // TODO: however - it's probably wiser to fall back on a simple static
        // evaluation if the `get()` returns `None`
        let score = self.tt.get(&self.pos).unwrap().score;

        // TODO: temp; replace with cleaner stuff.
        let best_move = self.get_best_move();
        match best_move {
            Some(mov) => {
                if self.sender.is_some() {
                    self.sender
                        .as_ref()
                        .unwrap()
                        .send(Message::FromEngine(Report::BestMove(mov.to_uci_string())));
                }
            }
            None => {}
        }

        score
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

        if self.is_halted() {
            // Engine received a halt command, and has set the halt flag to true.
            // We need to unwind from the search, transmit the best move found so far
            // through the channel and and return the current evaluation gracefully.

            // TODO: this isn't right. We can't return the current alpha. This function
            // to return a monad of some kind, which tells the caller whether we are
            // returning early.
            return alpha;
        }

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

    fn is_halted(&self) -> bool {
        *self.halt.read().unwrap()
    }

    fn report(&self, msg: Report) {
        match &self.sender {
            Some(sender) => {
                sender
                  .send(Message::FromSearch(msg))
                  .expect("Error: couldn't send report to GUI. No communication channel provided to search thread.");
            }
            None => {}
        }
    }
}
