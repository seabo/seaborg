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
use log::info;
use separator::Separatable;

use std::cmp::{max, min};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// The maximum ply to reach in the iterative deepening function.
static MAX_DEPTH_PLY: u8 = u8::MAX;

/// Represents the search mode specified in a `go` command. The `go` keyword
/// can either be followed by `infinite` which means the position should be
/// searched indefinitely in 'analysis mode', or it can be followed by a
/// string like `wtime 10000 btime 10000 winc 1000 binc 1000 movestogo 5`
/// which represents the clock situation in a timed game.
#[derive(Copy, Clone, Debug)]
pub enum SearchMode {
    /// Search the position indefinitely, until a `stop` command.
    Infinite,
    /// Contains information representing the clock situation for white and black.
    Timed(TimeControl),
    /// Contains a fixed amount of time, in milliseconds, in which to make the move.
    FixedTime(u32),
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

#[derive(Debug)]
pub struct Search {
    /// The internal board representation used by the search.
    pos: Position,
    /// The transposition table used by the search process.
    tt: Table<TTData>,
    /// A channel sender for transmitting messages back to the `Engine`. These messages are
    /// usually things like info reports or the result of a search.
    sender: Option<Sender<Message>>,
    /// A cross-thread boolean flag which is set to `true` by the `Engine` struct when it wants
    /// to halt the process from the outside. TODO: it's assumed that this is cheaper and
    /// lighterweight than setting up a channel into this thread and periodically calling `recv()`
    /// inside the search function to determine whether or not to stop.
    halt: Arc<RwLock<bool>>,
    /// The number of nodes we have visited in the search tree.
    visited: usize,
    /// A counter of the number of moves which have been considered from all nodes visited.
    moves_considered: usize,
    /// A counter of the number of edge traversals we have done in the search tree.
    moves_visited: usize,
    /// The time at which the search commenced.
    start_time: Option<Instant>,
    /// The amount of time, in milliseconds, to limit this search to.
    time_limit: Option<u32>,
    /// Tracks the best move found for the root position after the last complete iterative
    /// deepening iteration.
    best_so_far: Option<Move>,
}

impl Search {
    pub fn new(
        mut params: Params,
        sender: Option<Sender<Message>>,
        halt: Arc<RwLock<bool>>,
    ) -> Self {
        let pos = params.take_pos();

        let tt = Table::with_capacity(params.tt_cap);

        let time_limit = match params.search_mode {
            SearchMode::Infinite => None,
            SearchMode::Timed(tc) => Some(tc.to_fixed_time(pos.move_number(), pos.turn())),
            SearchMode::FixedTime(t) => Some(t),
        };

        info!("setting search time limit to: {:?}", time_limit);

        let search = Search {
            pos,
            tt,
            sender,
            halt,
            visited: 0,
            moves_considered: 0,
            moves_visited: 0,
            start_time: None,
            time_limit,
            best_so_far: None,
        };

        search
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

    pub fn iterative_deepening(&mut self) -> i32 {
        info!("starting iterative deepening");

        // Record the time we started the search
        self.set_start_time();

        for i in 1..MAX_DEPTH_PLY {
            info!("iterative deepening: ply {}", i);
            self.best_so_far = self.get_best_move();
            info!(
                "best move found so far: {}",
                match self.best_so_far {
                    Some(mov) => mov,
                    None => Move::null(),
                }
            );

            if self.is_halted() || self.timed_out() {
                break;
            }

            self.search(i, -10_000, 10_000);
        }

        // TODO: if the TT has a `None` score, we should at least fall back on
        // a static evaluation of the position.
        let score = match self.tt.get(&self.pos) {
            Some(entry) => entry.score,
            None => 0,
        };

        match self.best_so_far {
            Some(mov) => self.send_best_move(mov),
            None => {
                // TODO: shouldn't really unwrap here, because if we are in checkmate
                // or stalemate, then there won't be any legal moves and this will
                // return `None`.
                self.send_best_move(self.pos.random_move().unwrap());
            }
        }

        info!("exiting iterative deepening routine");

        score
    }

    fn send_best_move(&self, mov: Move) {
        self.report(Report::BestMove(mov.to_uci_string()));
    }

    fn set_start_time(&mut self) {
        self.start_time = Some(Instant::now());
    }

    fn timed_out(&self) -> bool {
        if self.start_time.is_none() || self.time_limit.is_none() {
            return false;
        }

        match self.start_time {
            Some(start) => match self.time_limit {
                Some(limit) => start.elapsed().as_millis() >= limit as u128,
                None => false,
            },
            None => false,
        }
    }

    fn search(&mut self, depth: u8, mut alpha: i32, mut beta: i32) -> i32 {
        let is_white = self.pos.turn().is_white();
        let alpha_orig = alpha;
        self.visited += 1;

        if self.pos.in_checkmate() {
            return -10_000;
        }

        // Check if we need to break out of the search because we were halted or
        // ran out of time.
        // Note: to avoid running this at every single node, we only do it at
        // leaf nodes, because we'll hit leaf nodes very frequently but we can
        // cut down on time wasted on the checks.
        if self.is_halted() || self.timed_out() {
            // We need to unwind from the search, transmit the best move found so far
            // through the channel and and return the current evaluation gracefully.
            // TODO: this isn't right. We can't return the current alpha. This function
            // to return a monad of some kind, which tells the caller whether we are
            // returning early.
            info!("timed-out/halted at depth {}", depth);

            return alpha;
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
                score = -self.search(depth - 1, -beta, -alpha);
            } else {
                score = -self.search(depth - 1, -alpha - 100, -alpha);
                if score > alpha {
                    // re-search
                    score = -self.search(depth - 1, -beta, -alpha);
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
