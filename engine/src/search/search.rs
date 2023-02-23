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

use super::ordering::{OrderedMoveList, OrderingPhase};
use super::params::Params;

use crate::engine::{Info, Report};
use crate::eval::material_eval;
use crate::sess::Message;
use crate::tables::Table;
use crate::time::TimeControl;

use core::mov::Move;
use core::position::Position;

use crossbeam_channel::Sender;
use log::info;
use separator::Separatable;

use std::cell::{Ref, RefCell, RefMut};
use std::cmp::{max, min};
use std::rc::Rc;
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
pub enum TimingMode {
    /// Search the position indefinitely, until a `stop` command.
    Infinite,
    /// Contains information representing the clock situation for white and black.
    Timed(TimeControl),
    /// Contains a fixed amount of time, in milliseconds, in which to make the move.
    FixedTime(u32),
    /// A specific search depth, measured in ply.
    Depth(u8),
}

// TODO: make the variants contain the value for cleaner access / manipulation.
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

impl TTData {
    pub fn best_move(&self) -> Move {
        self.best_move
    }
}

#[derive(Debug)]
pub struct Search {
    /// The internal board representation used by the search.
    pos: Rc<RefCell<Position>>,
    /// The transposition table used by the search process.
    tt: Rc<RefCell<Table<TTData>>>,
    /// A channel sender for transmitting messages back to the `Engine`. These messages are
    /// usually things like info reports or the result of a search.
    sender: Option<Sender<Message>>,
    /// A cross-thread boolean flag which is set to `true` by the `Engine` struct when it wants
    /// to halt the process from the outside. TODO: it's assumed that this is cheaper and
    /// lighterweight than setting up a channel into this thread and periodically calling `recv()`
    /// inside the search function to determine whether or not to stop.
    halt: Arc<RwLock<bool>>,
    /// The number of nodes entered by calling `search()`.
    visited: usize,
    /// A counter of the number of edge traversals we have done in the search tree.
    moves_visited: usize,
    /// Counts the total number of beta cutoffs which have occurred in the search.
    beta_cutoffs: usize,
    /// Counts the number of beta cutoffs which occurred when searching a transposition table move,
    /// and therefore before invoking a movegen for the current node (because of the lazy move iterator).
    beta_cutoffs_on_tt_move: usize,
    /// Counts the number of beta cutoffs which occurred when searching a capture move,
    /// and therefore before invoking the final movegen for the current node (because of the lazy move iterator).
    beta_cutoffs_on_captures: usize,
    /// The time at which the search commenced.
    start_time: Option<Instant>,
    /// The amount of time, in milliseconds, to limit this search to.
    time_limit: Option<u32>,
    /// Tracks the best move found for the root position after the last complete iterative
    /// deepening iteration.
    best_so_far: Option<Move>,
    /// Highest depth reached so far in iterative deepening.
    highest_depth: u8,
    /// The maximum search depth we are allowed to search to.
    max_depth: Option<u8>,
}

impl Search {
    pub fn new(
        mut params: Params,
        sender: Option<Sender<Message>>,
        halt: Arc<RwLock<bool>>,
    ) -> Self {
        let pos = Rc::new(RefCell::new(params.take_pos()));

        let tt = Rc::new(RefCell::new(Table::with_capacity(params.tt_cap)));

        let time_limit = match params.search_mode {
            TimingMode::Infinite => None,
            TimingMode::Timed(tc) => {
                Some(tc.to_fixed_time(pos.borrow().move_number(), pos.borrow().turn()))
            }
            TimingMode::FixedTime(t) => Some(t),
            TimingMode::Depth(d) => None,
        };

        let max_depth = match params.search_mode {
            TimingMode::Depth(d) => Some(d),
            _ => None,
        };

        info!("setting search time limit to: {:?}", time_limit);

        Search {
            pos,
            tt,
            sender,
            halt,
            visited: 0,
            moves_visited: 0,
            beta_cutoffs: 0,
            beta_cutoffs_on_tt_move: 0,
            beta_cutoffs_on_captures: 0,
            start_time: None,
            time_limit,
            best_so_far: None,
            highest_depth: 0,
            max_depth,
        }
    }

    #[inline(always)]
    pub fn pos(&self) -> Ref<'_, Position> {
        self.pos.borrow()
    }

    #[inline(always)]
    pub fn pos_mut(&self) -> RefMut<'_, Position> {
        self.pos.borrow_mut()
    }

    #[inline(always)]
    pub fn tt(&self) -> Ref<'_, Table<TTData>> {
        self.tt.borrow()
    }

    #[inline(always)]
    pub fn tt_mut(&self) -> RefMut<'_, Table<TTData>> {
        self.tt.borrow_mut()
    }

    pub fn display_trace(&self) {
        println!("Visited {} nodes", self.visited.separated_string());
        println!("Moves visited: {}", self.moves_visited.separated_string());
        self.tt().display_trace();
    }

    pub fn get_tt_move(&mut self) -> Option<Move> {
        match self.tt().get(&self.pos()) {
            Some(data) => Some(data.best_move),
            None => None,
        }
    }

    pub fn recover_pv(&mut self) -> Vec<Move> {
        let mut pv: Vec<Move> = Vec::new();
        let mut length = 0;
        while let Some(mov) = self.get_tt_move() {
            pv.push(mov);
            self.pos_mut().make_move(mov);
            length += 1;
        }

        while length > 0 {
            length -= 1;
            self.pos_mut().unmake_move();
        }

        pv
    }

    pub fn iterative_deepening(&mut self) -> i32 {
        info!("starting iterative deepening");

        // Record the time we started the search
        self.set_start_time();

        let target_depth = match self.max_depth {
            Some(d) => d,
            None => MAX_DEPTH_PLY,
        };

        for i in 1..=target_depth {
            info!("iterative deepening: ply {}", i);

            self.search(i, -10_000, 10_000);
            self.highest_depth = i;

            if self.is_halted() || self.timed_out() {
                break;
            } else {
                self.update_best_move();
                self.report_info();
            }
        }

        let tt_result = self.tt().get(&self.pos());
        let score = match tt_result {
            Some(entry) => entry.score,
            None => self.quiesce(-10_000, 10_000),
        };

        match self.best_so_far {
            Some(mov) => self.send_best_move(mov),
            None => {
                // TODO: shouldn't really unwrap here, because if we are in checkmate
                // or stalemate, then there won't be any legal moves and this will
                // return `None`.
                self.send_best_move(self.pos().random_move().unwrap());
            }
        }

        self.exit_search();

        score
    }

    fn exit_search(&mut self) {
        info!("exiting iterative deepening routine");
        info!(
            "search speed: {} NPS",
            self.visited as f32 / self.start_time.unwrap().elapsed().as_secs_f32()
        );
        info!("visited {} nodes in search", self.visited);
        info!("beta-cutoffs: {}", self.beta_cutoffs);
        info!("beta-cutoffs at tt moves: {}", self.beta_cutoffs_on_tt_move);
        info!(
            "{}% of beta-cutoffs at tt moves",
            self.beta_cutoffs_on_tt_move as f32 * 100 as f32 / self.beta_cutoffs as f32
        );
        info!(
            "beta-cutoffs at capture moves: {}",
            self.beta_cutoffs_on_captures
        );
        info!(
            "{}% of beta-cutoffs at capture moves",
            self.beta_cutoffs_on_captures as f32 * 100 as f32 / self.beta_cutoffs as f32
        );
    }

    fn update_best_move(&mut self) {
        self.best_so_far = self.get_tt_move();
        info!(
            "best move found so far: {}",
            match self.best_so_far {
                Some(mov) => mov,
                None => Move::null(),
            }
        );
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
        // let is_white = self.pos.turn().is_white();
        let alpha_orig = alpha;
        self.visited += 1;

        // Check if we need to break out of the search because we were halted or
        // ran out of time.
        // Note: to avoid running this at every single node, we only do it at
        // leaf nodes, because we'll hit leaf nodes very frequently.
        if self.is_halted() || self.timed_out() {
            // We need to unwind from the search, transmit the best move found so far
            // through the channel and and return the current evaluation gracefully.
            // TODO: this isn't right. We can't return the current alpha. This function
            // to return a monad of some kind, which tells the caller whether we are
            // returning early.
            info!("timed-out/halted at depth {}", depth);

            return alpha;
        }

        if let Some(data) = self.tt().get(&self.pos()) {
            if data.depth >= depth {
                match data.node_type {
                    NodeType::Exact => return data.score,
                    NodeType::LowerBound => alpha = max(alpha, data.score),
                    NodeType::UpperBound => beta = min(beta, data.score),
                }
            }
        };

        if depth == 0 {
            return self.quiesce(alpha, beta);
        }

        let mut best_move: Move = Move::null();
        let mut search_pv = true;
        let mut val = -10_000;
        let mut node_move_count = 0;
        let ordered_moves = OrderedMoveList::new(self.pos.clone(), self.tt.clone());
        for (mov, ordering_phase) in ordered_moves {
            node_move_count += 1;
            self.moves_visited += 1;
            self.pos_mut().make_move(mov);
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
            self.pos_mut().unmake_move();

            if score > val {
                val = score;
                search_pv = false;
                best_move = mov.clone();
            }
            alpha = max(alpha, val);

            if val >= beta {
                self.beta_cutoffs += 1;
                if ordering_phase == OrderingPhase::TTMove {
                    self.beta_cutoffs_on_tt_move += 1;
                } else if ordering_phase == OrderingPhase::Captures {
                    self.beta_cutoffs_on_captures += 1;
                }
                break;
            }
        }

        if node_move_count == 0 {
            // If this condition is true, then we had no moves in this position. So it's
            // either checkmate or stalemate.
            if self.pos().in_check() {
                // Checkmate
                return -10_000;
            } else {
                // Stalemate
                return 0;
            }
        }

        let node_type = if val <= alpha_orig {
            NodeType::UpperBound
        } else if val >= beta {
            NodeType::LowerBound
        } else {
            NodeType::Exact
        };

        if !best_move.is_null() {
            let tt_entry = TTData {
                depth,
                node_type,
                score: val,
                best_move,
            };
            self.tt_mut().insert(&self.pos(), tt_entry);
        }
        return val;
    }

    pub fn quiesce(&mut self, mut alpha: i32, beta: i32) -> i32 {
        let stand_pat = self.evaluate();
        if stand_pat >= beta {
            return beta;
        }

        if alpha < stand_pat {
            alpha = stand_pat;
        }

        let captures = self.pos().generate_captures();
        let mut score: i32;

        let mut node_move_count = 0;
        for mov in &captures {
            node_move_count += 1;
            self.pos_mut().make_move(*mov);
            score = -self.quiesce(-beta, -alpha);
            self.pos_mut().unmake_move();

            if score >= beta {
                return beta;
            }

            if score > alpha {
                alpha = score;
            }
        }

        if node_move_count == 0 {
            // If this condition is true, then we had no moves in this position. So it's
            // either checkmate or stalemate.
            if self.pos().in_check() {
                // Checkmate
                return -10_000;
            } else {
                // Stalemate
                return 0;
            }
        }

        alpha
    }

    pub fn evaluate(&mut self) -> i32 {
        material_eval(&self.pos()) * if self.pos().turn().is_white() { 1 } else { -1 }
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

    fn report_info(&self) {
        if let Some(info) = self.build_info() {
            self.report(Report::Info(info));
        }
    }

    fn build_info(&self) -> Option<Info> {
        match self.tt().get(&self.pos()) {
            Some(tt_entry) => {
                Some(Info {
                    depth: self.highest_depth,
                    // TODO: `seldepth` currently just same as `depth`
                    seldepth: self.highest_depth,
                    score: tt_entry.score,
                    nodes: self.visited,
                    nps: self.visited * 1_000_000
                        / self.start_time.unwrap().elapsed().as_micros() as usize,
                    pv: tt_entry.best_move.to_string(),
                })
            }
            None => None,
        }
    }
}
