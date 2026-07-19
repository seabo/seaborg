use crate::history::HistoryTable;

use super::eval::Evaluation;
use super::killer::KillerTable;
use super::ordering::{Loader, OrderedMoves, ScoredMoveList, Scorer};
use super::pv_table::PVTable;
use super::score::Score;
use super::trace::Tracer;
use super::tt::{Bound, Table};

use core::mono_traits::{All as AllGen, Captures, Legal, QueenPromotions, Quiets};
use core::mov::Move;
use core::movelist::{BasicMoveList, MoveList};
use core::position::{Player, Position};

use separator::Separatable;

use std::ops::Neg;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

const MAX_DEPTH: u8 = 255;

/// A node either completed with a usable score or aborted before establishing one.
type NodeResult = Option<Score>;

fn should_razor(depth: u8, eval: Score, alpha: Score) -> bool {
    depth <= 6 && alpha.is_cp() && eval + Score::cp(426 + 252 * depth as i16 * depth as i16) < alpha
}

/// A limit controlling how long a search may run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchLimit {
    /// Search through the given depth.
    Depth(u8),
    /// Search until the given amount of wall-clock time has elapsed.
    Time(Duration),
    /// Search until explicitly cancelled.
    Infinite,
}

/// A snapshot produced after an iterative-deepening iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchProgress {
    pub depth: u8,
    pub score: Score,
    pub elapsed: Duration,
    pub nodes: usize,
    pub nps: u32,
    pub hashfull: u16,
    pub principal_variation: Vec<Move>,
}

/// The move currently being considered at the root of the search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentMove {
    pub depth: u8,
    pub current_move: Move,
    pub number: u8,
}

/// A typed update emitted while a search is running.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchEvent {
    Progress(SearchProgress),
    CurrentMove(CurrentMove),
}

/// The final result from a completed iterative-deepening iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub score: Score,
    pub best_move: Option<Move>,
    pub depth: u8,
}

/// The reason a search stopped, together with its latest completed result, if any.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchOutcome {
    Completed(Option<SearchResult>),
    Cancelled(Option<SearchResult>),
}

impl SearchOutcome {
    pub fn result(&self) -> Option<&SearchResult> {
        match self {
            Self::Completed(result) | Self::Cancelled(result) => result.as_ref(),
        }
    }

    pub fn was_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled(_))
    }
}

/// A clonable token used to cancel a running search.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A reusable owner of search resources.
pub struct SearchEngine {
    table: Arc<Table>,
}

impl SearchEngine {
    pub fn new(hash_size_mb: usize) -> Self {
        Self {
            table: Arc::new(Table::new(hash_size_mb)),
        }
    }

    /// Invalidate the shared hash at an explicit administrative boundary.
    ///
    /// Callers own this operation and must ensure no active searches need the current generation.
    pub fn clear_hash(&self) {
        self.table.clear();
    }

    /// Begin a new game with a fresh transposition-table generation.
    ///
    /// Normal searches reuse the current generation; only the session owner starts a new one.
    pub fn new_game(&self) {
        self.clear_hash();
    }

    /// Start searching a cloned position on a background thread.
    pub fn start(&self, position: Position, limit: SearchLimit) -> SearchHandle {
        self.start_inner(position, limit).0
    }

    /// Start a search while also handing back a clone of the worker's event `Sender`.
    ///
    /// Production callers use [`SearchEngine::start`] and drop the extra sender
    /// immediately. Tests retain it to hold the events channel open, which lets them
    /// assert that completion is observed through the explicit signal rather than
    /// through a channel disconnect.
    fn start_inner(
        &self,
        position: Position,
        limit: SearchLimit,
    ) -> (SearchHandle, Sender<SearchEvent>) {
        if let SearchLimit::Depth(depth) = limit {
            assert!(depth > 0, "search depth must be greater than zero");
        }

        let cancellation = CancellationToken::new();
        let thread_cancellation = cancellation.clone();
        let table = Arc::clone(&self.table);
        let (events, receiver) = unbounded();
        let events_probe = events.clone();
        // Capacity 1 and a single send per worker, so signalling completion can never
        // block the worker thread on its way out.
        let (finished_tx, finished_rx) = bounded(1);
        let join = std::thread::spawn(move || {
            let (depth, deadline) = match limit {
                SearchLimit::Depth(depth) => (depth, None),
                SearchLimit::Time(duration) => (MAX_DEPTH, Some(Instant::now() + duration)),
                SearchLimit::Infinite => (MAX_DEPTH, None),
            };
            let mut search =
                Search::with_events(position, &thread_cancellation.0, deadline, &table, events);
            let result = search.run::<Master>(depth);
            let outcome = if thread_cancellation.is_cancelled() {
                SearchOutcome::Cancelled(result)
            } else {
                SearchOutcome::Completed(result)
            };
            // Release the event `Sender` before signalling, so a driver woken by the
            // signal finds the full event backlog already queued and terminated.
            drop(search);
            // The explicit completion signal. The driver must never have to infer that
            // this thread finished from the events channel disconnecting: that wakeup
            // has been observed to be lost, parking the driver forever (TASK-35).
            let _ = finished_tx.send(());
            outcome
        });

        let handle = SearchHandle {
            cancellation,
            events: receiver,
            finished: finished_rx,
            join: Some(join),
        };
        (handle, events_probe)
    }

    /// Test-only variant of [`SearchEngine::start`] that keeps the worker's event
    /// `Sender` alive, so the events channel never disconnects when the worker exits.
    #[cfg(test)]
    pub(crate) fn start_retaining_events(
        &self,
        position: Position,
        limit: SearchLimit,
    ) -> (SearchHandle, Sender<SearchEvent>) {
        self.start_inner(position, limit)
    }
}

/// Access to a running search's events, cancellation, and final outcome.
pub struct SearchHandle {
    cancellation: CancellationToken,
    events: Receiver<SearchEvent>,
    finished: Receiver<()>,
    join: Option<JoinHandle<SearchOutcome>>,
}

impl SearchHandle {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn events(&self) -> &Receiver<SearchEvent> {
        &self.events
    }

    /// Receives exactly one message once the worker thread has finished, whether the
    /// search completed or was cancelled.
    ///
    /// This is the authoritative completion signal. Unlike the events channel
    /// disconnecting, it is an ordinary message send on a channel the driver is
    /// already selecting over.
    pub fn finished(&self) -> &Receiver<()> {
        &self.finished
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn is_finished(&self) -> bool {
        self.join.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn wait(mut self) -> SearchOutcome {
        self.join
            .take()
            .expect("search outcome was already taken")
            .join()
            .expect("search thread panicked")
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.cancel();
        }
    }
}

/// Trait to monomorphize search functionality over different thread types: master and worker.
///
/// The master thread emits typed search events while workers search silently.
pub trait Thread {
    fn is_master() -> bool;
}

/// Dummy type representing the master search thread.
pub struct Master;
impl Thread for Master {
    fn is_master() -> bool {
        true
    }
}

/// Dummy type representing a worker thread.
pub struct Worker;
impl Thread for Worker {
    fn is_master() -> bool {
        false
    }
}

/// Trait to monomorphize search routine over the node type.
///
/// The three node types are PV, ALL and CUT.
///
/// * The root node is a PV node.
/// * The first child of a PV node is a PV node.
/// * Children of PV nodes that are searched with a zero-window are Cut nodes.
/// * Children of PV nodes that have to be re-search because the scout search failed high are PV
///   nodes.
/// * The first child of a Cut node and other candidate cutoff moves (nullmove, killers, captures,
///   checks) is an All node.
/// * A Cut node becomes an All node once all the candidate cutoff moves are searched.
/// * Children of All nodes are Cut nodes.
pub trait NodeType {
    fn pv() -> bool;
    fn cut() -> bool;
    fn all() -> bool;
    fn root() -> bool;
}

/// Dummy type representing a PV node.
pub struct Pv;
impl NodeType for Pv {
    fn pv() -> bool {
        true
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing a non-PV node.
pub struct NonPv;
impl NodeType for NonPv {
    fn pv() -> bool {
        false
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing a CUT node.
pub struct Cut;
impl NodeType for Cut {
    fn pv() -> bool {
        false
    }
    fn cut() -> bool {
        true
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing an ALL node.
pub struct All;
impl NodeType for All {
    fn pv() -> bool {
        false
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        true
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing the root node. This is also a PV node.
pub struct Root;
impl NodeType for Root {
    fn pv() -> bool {
        true
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        true
    }
}

/// Manages the search.
pub struct Search<'engine> {
    /// The internal board position.
    pub(super) pos: Position,
    /// Table for tracking the principal variation of the search.
    pvt: PVTable,
    /// Tracer to track search stats.
    trace: Tracer,
    /// The transposition table.
    tt: &'engine Table,
    /// The killer move table.
    kt: KillerTable,
    /// The history table.
    history: HistoryTable,
    /// Flag to indicate when the search should start unwinding due to user intervention.
    stopping: &'engine AtomicBool,
    /// Time to at which to end search.
    stop_time: Option<Instant>,
    /// Whether the guaranteed-minimum search (one full ply) has completed. Both abort signals (the
    /// cancellation flag and the time deadline) are suppressed until this is set, so a search
    /// always returns a completed legal root move whenever one exists, even when the allotted
    /// budget is zero or already elapsed or an immediate stop arrives.
    min_search_complete: bool,
    #[cfg(test)]
    abort_after_nodes: Option<usize>,
    /// Destination for typed search progress events.
    events: Option<Sender<SearchEvent>>,
    search_depth: u8,
    depth_reached: u8,
}

impl<'engine> Search<'engine> {
    pub fn new(
        pos: Position,
        flag: &'engine AtomicBool,
        stop_time: Option<Instant>,
        tt: &'engine Table,
    ) -> Self {
        Self::build(pos, flag, stop_time, tt, None)
    }

    fn with_events(
        pos: Position,
        flag: &'engine AtomicBool,
        stop_time: Option<Instant>,
        tt: &'engine Table,
        events: Sender<SearchEvent>,
    ) -> Self {
        Self::build(pos, flag, stop_time, tt, Some(events))
    }

    fn build(
        pos: Position,
        flag: &'engine AtomicBool,
        stop_time: Option<Instant>,
        tt: &'engine Table,
        events: Option<Sender<SearchEvent>>,
    ) -> Self {
        Self {
            pos,
            tt,
            kt: KillerTable::new(20),
            history: HistoryTable::new(),
            pvt: PVTable::new(8),
            trace: Tracer::new(),
            stopping: flag,
            stop_time,
            events,
            search_depth: 0,
            depth_reached: 0,
            min_search_complete: false,
            #[cfg(test)]
            abort_after_nodes: None,
        }
    }

    pub fn run<T: Thread>(&mut self, d: u8) -> Option<SearchResult> {
        self.trace = Tracer::new();

        assert!(d > 0);

        // Some bookeeping and prep.
        let start_zob = self.pos.zobrist();

        self.trace.commence_search();
        self.search_depth = d;
        self.min_search_complete = false;

        let result = self.iterative_deepening::<T>(d);
        self.trace.end_search();

        assert_eq!(start_zob, self.pos.zobrist());

        if let Some(result) = &result {
            self.report_telemetry(d, result.score);
        }

        self.history.reset();

        result
    }

    fn iterative_deepening<T: Thread>(&mut self, depth: u8) -> Option<SearchResult> {
        let mut result = None;

        for d in 1..=depth {
            if self.stopping() {
                break;
            }

            let completed_pvt = std::mem::replace(&mut self.pvt, PVTable::new(d));
            self.search_depth = d;
            let Some(value) = self.search::<T, Root>(Score::INF_N, Score::INF_P, d) else {
                self.pvt = completed_pvt;
                break;
            };

            self.depth_reached = d;
            result = Some(SearchResult {
                score: value,
                best_move: self.pvt.pv().next().copied(),
                depth: d,
            });
            if T::is_master() {
                self.emit_progress(d, value);
            }

            // The first full ply is guaranteed to run to completion; from here on the time-based
            // deadline is honored so deeper iterations respect the allotted clock.
            self.min_search_complete = true;
        }

        result
    }

    pub fn search<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        mut beta: Score,
        depth: u8,
    ) -> NodeResult {
        self.trace.visit_node();

        let draft = self.search_depth - depth;
        let mut tt_move = false;

        // The PV row for this ply is rebuilt from scratch on every visit, so clear it before any
        // early return can leave a previously searched sibling's line in place for this node's
        // parent to splice into its own PV. See `PVTable::clear_at`.
        self.pvt.clear_at(depth);

        debug_assert!(Score::INF_N <= alpha);
        debug_assert!(alpha < beta);
        debug_assert!(beta <= Score::INF_P);
        debug_assert!(Node::pv() || alpha.inc_one() == beta);

        // Step 1. Check for aborted search and immediate draw.
        if self.stopping() {
            return None;
        }

        // Step 2. check for immediate draw.
        if self.pos.in_threefold() || self.pos.fifty_move_rule_reached() {
            return Some(Score::zero());
        }

        // Step 2. Mate distance pruning.
        if !Node::root() {
            // Scores are position-relative: at every node, being checkmated now and mating on the
            // next ply are the worst and best possible mate values. Using `draft` here treats the
            // bounds as root-relative; those wrong-parity bounds can escape a cutoff and masquerade
            // as an exact root result.
            alpha = std::cmp::max(Score::mate(0), alpha);
            beta = std::cmp::min(Score::mate(1), beta);
            if alpha >= beta {
                return Some(alpha);
            }
        }

        // Step 3. Load transposition table entry.
        let (tt_entry, tt_mov) = {
            use super::tt::Probe::*;
            match self.tt.probe(&self.pos) {
                Hit(entry) => {
                    let e = entry.read();
                    if e.mov.is_null() {
                        (entry, None)
                    } else {
                        let mov = e.mov.to_move(&self.pos);
                        if self.pos.valid_move(&mov) {
                            self.trace.hash_hit();
                            tt_move = true;
                            (entry, Some(mov))
                        } else {
                            self.trace.hash_collision();
                            (entry, None)
                        }
                    }
                }
                Clash(entry) => {
                    self.trace.hash_clash();
                    (entry, None)
                }
                Empty(entry) => (entry, None),
            }
        };

        // Step 4. Check for early cutoff.
        if !Node::pv() && tt_move {
            let entry = tt_entry.read();

            if !entry.is_empty() && entry.depth >= depth {
                match entry.bound() {
                    Bound::Exact => {
                        return Some(entry.score);
                    }
                    Bound::Lower => {
                        if entry.score > beta {
                            return Some(entry.score);
                        } else if entry.score > alpha {
                            alpha = entry.score
                        }
                    }
                    Bound::Upper => {
                        if entry.score < alpha {
                            return Some(entry.score);
                        } else if entry.score < beta {
                            beta = entry.score
                        }
                    }
                }
            }

            if alpha == beta {
                return Some(alpha);
            }
        }

        // Step 5. Straight to quiescence search if depth <= 0.
        if depth == 0 {
            return self.quiesce::<T, Node>(alpha, beta);
        }

        // Step 6. Static evaluation.
        let eval = self.evaluate();

        // Step 7. Razoring.
        // When eval is very low, check with quiescence whether it has any hope of raising alpha. If
        // not, return a fail low.
        if should_razor(depth, eval, alpha) {
            let value = self.quiesce::<Master, NonPv>(alpha - Score::cp(1), alpha)?;
            if value < alpha {
                return Some(value);
            }
        }

        // Step 8. Futility pruning.
        //         TODO

        // Step 9. Null move search with verification (non-PV only).
        //         TODO

        // Step 10. ProbCut.
        //         TODO

        // Step 11. In PV nodes, if the move is not in TT, decrease depth by 3.
        //          TODO

        // Step 12. If depth <= 0, run quiescence search.
        //          Handled earlier, at Step 5.

        // Step 13. In non-PV nodes with depth >= 7 and not in TT, decrease depth by 2.
        //          TODO

        // Step 14. If PV move and TT move failed low, this is a likely fail-low.
        //          TODO

        // Step 15. Iterate moves.
        let mut best_value = Score::INF_N;
        let mut best_move = Move::null();
        let mut moves = OrderedMoves::new();
        let mut move_count = 0;
        let mut did_raise_alpha = false;

        'move_loop: while moves.load_next_phase(MoveLoader::from(self, tt_mov, draft)) {
            for mov in &moves {
                if self.stopping() {
                    break 'move_loop;
                }

                move_count += 1;
                let mut value = Score::INF_N;

                // Start reporting which move we're considering after 3 seconds have elapsed.
                if T::is_master() && Node::root() && self.trace.live_elapsed().as_millis() > 3000 {
                    self.emit_current_move(depth, mov, move_count);
                }

                // Step 16. Reductions & extensions.
                //          TODO

                // Step 17. Late move reduction.
                //          TODO

                // Step 18. Make the move.
                // SAFETY: ordered moves originate from move generation for `self.pos`.
                unsafe { self.pos.make_move_unchecked(mov) };

                // Step 19. Search non-PV move with null window.
                if !Node::pv() || move_count > 1 {
                    let child = self.search::<T, NonPv>(
                        alpha.inc_one().child_bound(),
                        alpha.child_bound(),
                        depth - 1,
                    );
                    let Some(child) = child else {
                        self.pos.unmake_move();
                        return None;
                    };
                    value = child.neg().inc_mate();
                }

                // Step 20. Search PV move, or perform re-search if null window search failed high.
                //
                // If this is a PV node, do a full search on the first move and any move for which
                // the null-window search failed to produce a cutoff.
                if Node::pv()
                    && (move_count == 1 || (value > alpha && (Node::root() || value < beta)))
                {
                    let child =
                        self.search::<T, Pv>(beta.child_bound(), alpha.child_bound(), depth - 1);
                    let Some(child) = child else {
                        self.pos.unmake_move();
                        return None;
                    };
                    value = child.neg().inc_mate();
                }

                debug_assert!(Node::pv() || !(value > alpha && (Node::root() || value < beta)));

                // Step 21. Undo move.
                self.pos.unmake_move();

                debug_assert!(value > Score::INF_N);
                debug_assert!(value < Score::INF_P);

                // Step 22. Check for new best move.
                if value > best_value {
                    best_value = value;

                    if value > alpha {
                        best_move = *mov;

                        if Node::pv() && value < beta {
                            // Only an exact score at a PV node establishes a variation worth
                            // reporting. A fail-high returns a lower bound whose "best" move was
                            // never searched with a full window, so publishing it would splice a
                            // non-PV continuation into the reported line. The root always lands
                            // here: its beta is `INF_P` and `value` is asserted below it.
                            self.pvt.copy_to(depth, *mov);

                            alpha = value;
                            did_raise_alpha = true;
                            // TODO: reduce depth on remaining moves.
                        } else {
                            debug_assert!(value >= beta);
                            // beta-cutoff; record killer and history
                            if mov.is_quiet() {
                                self.kt.store(*mov, draft);
                            }

                            // self.history.inc(
                            //     mov.orig(),
                            //     mov.dest(),
                            //     depth as u32 * depth as u32,
                            //     self.pos.turn(),
                            // );

                            break 'move_loop;
                        }
                    }
                }
            }
        }

        if self.stopping() {
            return None;
        }

        debug_assert!(
            move_count > 0
                || self
                    .pos
                    .generate::<BasicMoveList, AllGen, Legal>()
                    .is_empty()
        );

        // Step 23. Check for mate and stalemate.
        if move_count == 0 {
            // The row was already emptied on entry, so this terminal node reports no continuation.
            best_value = if self.pos.in_check() {
                Score::mate(0)
            } else {
                Score::cp(0)
            };
        }

        debug_assert!(best_value > Score::INF_N);

        // Step 24. Write node information to the transposition table.
        tt_entry.write(
            &self.pos,
            best_value,
            depth,
            if best_value >= beta {
                debug_assert!(
                    !best_move.is_null()
                        || best_value == Score::mate(0)
                        || best_value == Score::zero()
                );
                Bound::Lower
            } else if Node::pv() && !best_move.is_null() {
                debug_assert!(did_raise_alpha);
                Bound::Exact
            } else {
                debug_assert!(!did_raise_alpha);
                Bound::Upper
            },
            &best_move,
        );

        // Step 25. Return best value.
        Some(best_value)
    }

    #[inline(always)]
    fn stopping(&self) -> bool {
        // The guaranteed-minimum search (the first full ply) always runs to completion so a legal
        // root move is available to return. Until it completes, neither the cancellation flag nor
        // the time deadline may abort the search; this prevents `bestmove 0000` forfeits at
        // zero/near-zero budgets and on an immediate `stop`. The first ply is finite, so this can
        // never hang.
        if !self.min_search_complete {
            return false;
        }

        #[cfg(test)]
        if self
            .abort_after_nodes
            .is_some_and(|limit| self.trace.all_nodes_visited() >= limit)
        {
            return true;
        }

        self.stopping.load(Ordering::Relaxed)
            || self
                .stop_time
                .map(|s| s <= std::time::Instant::now())
                .unwrap_or(false)
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    #[inline(always)]
    fn evaluate(&mut self) -> Score {
        let material = (self.pos.material_eval() * self.pov()) as f32;
        let threshold = Position::FIFTY_MOVE_RULE_PLIES;
        let remaining = threshold - std::cmp::min(self.pos.half_move_clock(), threshold);
        let hmc = remaining as f32 / threshold as f32;
        let scaled_material = (material * hmc).round() as i16;
        Score::cp(scaled_material)
    }

    /// Returns 1 if the player to move is White, -1 if Black. Useful wherever we are using
    /// evaluation functions in a negamax framework, and have to return the evaluation from the
    /// perspective of the side to move.
    #[inline(always)]
    fn pov(&self) -> i16 {
        match self.pos.turn() {
            Player::WHITE => 1,
            Player::BLACK => -1,
        }
    }

    /// The quiescence search.
    fn quiesce<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        mut beta: Score,
    ) -> NodeResult {
        self.trace.visit_q_node();

        debug_assert!(!Node::root());
        debug_assert!(Score::INF_N <= alpha);
        debug_assert!(alpha < beta);
        debug_assert!(beta <= Score::INF_P);
        debug_assert!(Node::pv() || alpha.inc_one() == beta);

        if self.stopping() {
            return None;
        }

        // Step 1. Check for an immediate draw. Quiet check evasions can repeat positions, so this
        // must happen before following another evasion.
        if self.pos.in_threefold() || self.pos.half_move_clock() >= 50 {
            return Some(Score::zero());
        }

        // Step 2. Load transposition table entry.
        let (tt_entry, tt_hit) = {
            use super::tt::Probe::*;
            match self.tt.probe(&self.pos) {
                Hit(entry) => {
                    self.trace.hash_hit();
                    (entry, true)
                }
                Clash(entry) => {
                    self.trace.hash_clash();
                    (entry, false)
                }
                Empty(entry) => (entry, false),
            }
        };

        // Step 3. Check for early TT cutoff.
        if !Node::pv() {
            let entry = tt_entry.read();

            // A quiescence node has depth zero, so results from quiescence or any deeper main
            // search are sufficiently deep. The stored score remains an alpha-beta bound; it is
            // never a replacement for the position's static evaluation.
            if tt_hit && !entry.is_empty() {
                match entry.bound() {
                    Bound::Exact => {
                        return Some(entry.score);
                    }
                    Bound::Lower => {
                        if entry.score >= beta {
                            return Some(entry.score);
                        } else if entry.score > alpha {
                            alpha = entry.score
                        }
                    }
                    Bound::Upper => {
                        if entry.score <= alpha {
                            return Some(entry.score);
                        } else if entry.score < beta {
                            beta = entry.score
                        }
                    }
                }
            }

            if alpha >= beta {
                return Some(alpha);
            }
        }

        let in_check = self.pos.in_check();

        // Step 4. Static evaluation. Stand pat is not a legal option while in check.
        if !in_check {
            let stand_pat = self.evaluate();

            if stand_pat >= beta {
                return Some(beta);
            }

            if alpha < stand_pat {
                alpha = stand_pat;
            }
        }

        let mut score: Score;
        if in_check {
            let moves = self.pos.generate::<BasicMoveList, AllGen, Legal>();
            return self.quiesce_evasions::<T, Node>(alpha, beta, &moves);
        }

        // Step 5. Loop through all the moves until no moves remain or a beta cutoff occurs.
        let mut moves = OrderedMoves::new();
        'move_loop: while moves.load_next_phase(QMoveLoader::from(self)) {
            for mov in &moves {
                if self.stopping() {
                    break 'move_loop;
                }

                // SAFETY: quiescence moves originate from move generation for `self.pos`.
                unsafe { self.pos.make_move_unchecked(mov) };
                let child = self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound());
                self.pos.unmake_move();
                score = child?.neg().inc_mate();

                if score >= beta {
                    return Some(beta);
                }

                if score > alpha {
                    alpha = score;
                }
            }
        }

        if self.stopping() {
            None
        } else {
            Some(alpha)
        }
    }

    fn quiesce_evasions<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        beta: Score,
        moves: &BasicMoveList,
    ) -> NodeResult {
        if moves.is_empty() {
            return Some(Score::mate(0));
        }

        for mov in moves {
            if self.stopping() {
                return None;
            }

            self.pos.make_move(mov);
            let child = self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound());
            self.pos.unmake_move();
            let score = child?.neg().inc_mate();

            if score >= beta {
                return Some(beta);
            }

            if score > alpha {
                alpha = score;
            }
        }

        Some(alpha)
    }

    fn emit_progress(&self, depth: u8, score: Score) {
        self.emit(SearchEvent::Progress(SearchProgress {
            depth,
            score,
            elapsed: self.trace.live_elapsed(),
            nodes: self.trace.nodes_visited(),
            principal_variation: self.pvt.pv().copied().collect(),
            hashfull: self.tt.hashfull(),
            nps: self.trace.live_nps() as u32,
        }));
    }

    fn emit_current_move(&self, depth: u8, mov: &Move, num: u8) {
        self.emit(SearchEvent::CurrentMove(CurrentMove {
            depth,
            current_move: *mov,
            number: num,
        }));
    }

    fn emit(&self, event: SearchEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }

    /// Detailed debug info about the search, printed after the end of search in debug mode.
    fn report_telemetry(&self, depth: u8, score: Score) {
        if false {
            println!(
                "nodes:     {}",
                self.trace.all_nodes_visited().separated_string()
            );
            println!(
                "% q_nodes: {:.2}%",
                self.trace.q_nodes_visited() as f32 / self.trace.all_nodes_visited() as f32 * 100.0
            );
            println!(
                "nps:       {}",
                self.trace
                    .nps()
                    .expect("`end_search` was called, so this should always work")
                    .separated_string()
            );
            println!(
                "see skips: {}",
                self.trace.see_skipped_nodes().separated_string()
            );
            println!(
                "time:      {}ms",
                self.trace
                    .elapsed()
                    .expect("we called `end_search`")
                    .as_millis()
                    .separated_string()
            );
            println!(
                "eff. bf:   {}",
                self.trace.eff_branching(depth).separated_string()
            );
            println!("tt stats ----------------");
            println!(
                " size: {}MB, slots: {}",
                self.tt.capacity_mb(),
                self.tt.capacity_entries().separated_string()
            );
            println!(
                " hits:       {:>8} ({:.1}%)",
                self.trace.hash_hits().separated_string(),
                self.trace.hash_hits() as f64 / self.trace.hash_probes() as f64 * 100.
            );
            println!(
                " collisions: {:>8} ({:.1}%)",
                self.trace.hash_collisions().separated_string(),
                self.trace.hash_collisions() as f64 / self.trace.hash_probes() as f64 * 100.
            );
            println!(
                " clashes:    {:>8} ({:.1}%)",
                self.trace.hash_clashes().separated_string(),
                self.trace.hash_clashes() as f64 / self.trace.hash_probes() as f64 * 100.
            );
            println!(" hashfull: {:.2}%", self.tt.hashfull() as f64 / 10.);
            println!("-------------------------");
            println!(
                "pv:        {}",
                self.pvt
                    .pv()
                    .map(|m| m.to_uci_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            );
            println!("score:     {:?}", score);
            println!(
                "tt move found at {:.2}% of nodes",
                self.trace.hash_found.avg() * 100_f64
            );
            println!(
                "killers found per node: {:.2}",
                self.trace.killers_per_node.avg() * 2_f64
            );
        }
    }
}

pub struct MoveLoader<'a, 'search> {
    search: &'a mut Search<'search>,
    hash_move: Option<Move>,
    draft: u8,
}

impl<'a, 'engine> MoveLoader<'a, 'engine> {
    /// Create a `MoveLoader` from the passed `Search`.
    #[inline(always)]
    pub fn from(search: &'a mut Search<'engine>, hash_move: Option<Move>, draft: u8) -> Self {
        MoveLoader {
            search,
            hash_move,
            draft,
        }
    }
}

impl<'a, 'search> Loader for MoveLoader<'a, 'search> {
    #[inline]
    fn load_hash(&mut self, movelist: &mut ScoredMoveList) {
        match self.hash_move {
            Some(mv) => {
                self.search.trace.hash_found.push(1);
                movelist.push(mv)
            }
            None => {
                self.search.trace.hash_found.push(0);
            }
        }
    }

    fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
        self.search
            .pos
            .generate_in::<_, QueenPromotions, Legal>(movelist);
    }

    fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Captures, Legal>(movelist);
    }

    fn load_killers(&mut self, movelist: &mut ScoredMoveList) {
        let (km1, km2) = self.search.kt.probe(self.draft, &self.search.pos);
        let mut cnt = 0;

        if let Some(km) = km1 {
            cnt += 1;
            movelist.push(km);
        }
        if let Some(km) = km2 {
            cnt += 1;
            movelist.push(km);
        }
        self.search.trace.killers_per_node.push_many(cnt, 2);
    }

    fn load_quiets(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Quiets, Legal>(movelist);
    }

    fn score_captures(&mut self, captures: Scorer) {
        for (mov, score) in captures {
            if mov.is_capture() {
                *score = self
                    .search
                    .see(
                        mov.orig(),
                        mov.dest(),
                        self.search.pos.piece_at_sq(mov.dest()).type_of(),
                        self.search.pos.piece_at_sq(mov.orig()).type_of(),
                    )
                    .to_i16();
            }
        }
    }

    fn score_quiets(&mut self, quiets: Scorer) {
        let turn = self.search.pos.turn();
        for (mov, score) in quiets {
            // SAFETY: these are legal moves, so both squares are valid.
            unsafe {
                *score = self
                    .search
                    .history
                    .get_unchecked(mov.orig(), mov.dest(), turn) as i16;
            }
        }
    }
}

/// Move loader for the quiescence search.
pub struct QMoveLoader<'a, 'search> {
    search: &'a mut Search<'search>,
}

impl<'a, 'engine> QMoveLoader<'a, 'engine> {
    /// Create a `MoveLoader` from the passed `Search`.
    #[inline(always)]
    pub fn from(search: &'a mut Search<'engine>) -> Self {
        QMoveLoader { search }
    }
}

impl<'a, 'search> Loader for QMoveLoader<'a, 'search> {
    fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
        self.search
            .pos
            .generate_in::<_, QueenPromotions, Legal>(movelist);
    }

    fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Captures, Legal>(movelist);
    }

    fn load_quiets(&mut self, movelist: &mut ScoredMoveList) {
        if self.search.pos.in_check() {
            self.search.pos.generate_in::<_, Quiets, Legal>(movelist);
        }
    }

    fn score_captures(&mut self, captures: Scorer) {
        for (mov, score) in captures {
            if mov.is_capture() {
                *score = self
                    .search
                    .see(
                        mov.orig(),
                        mov.dest(),
                        self.search.pos.piece_at_sq(mov.dest()).type_of(),
                        self.search.pos.piece_at_sq(mov.orig()).type_of(),
                    )
                    .to_i16();
            }
        }
    }

    fn score_quiets(&mut self, quiets: Scorer) {
        let turn = self.search.pos.turn();
        for (mov, score) in quiets {
            // SAFETY: these are legal moves, so both squares are valid.
            unsafe {
                *score = self
                    .search
                    .history
                    .get_unchecked(mov.orig(), mov.dest(), turn) as i16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[rustfmt::skip]
    fn suite() -> Vec<(&'static str, u8, Score, Score, &'static str)> {
        // Test position tuples have the form:
        // (fen, depth, score range, best_move)

        vec![
                // Mates
                ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6, Score::mate(5), Score::mate(5), "c7c6"),
                ("5R2/1p1r2pk/p1n1B2p/2P1q3/2Pp4/P6b/1B1P4/2K3R1 w - - 5 3", 6, Score::mate(5), Score::mate(5), "e6g8"),
                ("1r6/p5pk/1q1p2pp/3P3P/4Q1P1/3p4/PP6/3KR3 w - - 0 36", 6, Score::mate(5), Score::mate(5), "h5g6"),
                ("1r4k1/p3p1bp/5P1r/3p2Q1/5R2/3Bq3/P1P2RP1/6K1 b - - 0 33", 6, Score::mate(5), Score::mate(5), "b8b1"),
                ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 6, Score::mate(5), Score::mate(5), "d3d7"),
                ("5rk1/rb3ppp/p7/1pn1q3/8/1BP2Q2/PP3PPP/3R1RK1 w - - 7 21", 6, Score::mate(5), Score::mate(5), "f3f7"),
                ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 8, Score::mate(7), Score::mate(7), "g4g2"),
                ("6k1/1p2qppp/4p3/8/p2PN3/P5QP/1r4PK/8 w - - 0 40", 6, Score::mate(5), Score::mate(5), "e4f6"),
                ("2R1bk2/p5pp/5p2/8/3n4/3p1B1P/PP1q1PP1/4R1K1 w - - 0 27", 6, Score::mate(5), Score::mate(5), "c8e8"),
                ("8/7R/r4pr1/5pkp/1R6/P5P1/5PK1/8 w - - 0 42", 6, Score::mate(5), Score::mate(5), "h7h5"),
                ("r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25", 8, Score::mate(7), Score::mate(7), "h5f7"),

                // // Winning material
                ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 7, Score::cp(290), Score::cp(310), "g7g6"),
                ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, Score::cp(100), Score::cp(100), "g1e1"), 
                ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 3, Score::cp(850), Score::cp(950), "d3c4"),
                ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6, Score::cp(900), Score::cp(900), "d2d8"),
                ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 4, Score::cp(380), Score::cp(420), "d1b3"),
                ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 5, Score::cp(380), Score::cp(420), "g7h6"),
                ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 6, Score::cp(0), Score::cp(0), "c7h7"),

                // Pawn race
                ("8/6pk/8/8/8/8/P7/K7 w - - 0 1", 22, Score::cp(450), Score::cp(920), "a1b1"),
        ]
    }

    /// Razoring relies on a static centipawn evaluation, so mate and infinity bounds are excluded.
    #[test]
    fn razoring_only_applies_to_centipawn_bounds() {
        assert!(should_razor(1, Score::cp(-1_000), Score::cp(0)));
        assert!(!should_razor(1, Score::cp(-1_000), Score::mate(5)));
        assert!(!should_razor(1, Score::cp(-1_000), Score::INF_P));
    }

    #[test]
    fn fifty_move_rule_uses_halfmove_boundary() {
        core::init::init_globals();

        for (halfmove_clock, expected) in [(99, false), (100, true), (101, true)] {
            let fen = format!("4k3/8/8/8/8/8/P7/Q3K3 w - - {halfmove_clock} 1");
            let pos = Position::from_fen(&fen).unwrap();
            assert_eq!(pos.fifty_move_rule_reached(), expected);

            let flag = AtomicBool::new(false);
            let tt = Table::new(1);
            let mut search = Search::new(pos, &flag, None, &tt);
            let result = search.run::<Master>(1).unwrap();
            assert_eq!(result.score == Score::zero(), expected);
        }
    }

    #[test]
    fn quiescence_searches_quiet_check_evasions() {
        core::init::init_globals();

        let position = Position::from_fen("k3r3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(position, &flag, None, &table);

        let score = search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P);

        assert_eq!(score, Some(Score::cp(-495)));
        assert!(search.trace.q_nodes_visited() > 1);
    }

    #[test]
    fn quiescence_detects_checkmate_at_the_horizon() {
        core::init::init_globals();

        let position = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(position, &flag, None, &table);

        assert_eq!(
            search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P),
            Some(Score::mate(0))
        );
    }

    #[test]
    fn quiescence_abort_with_legal_evasions_is_not_checkmate() {
        core::init::init_globals();

        let position = Position::from_fen("k3r3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let moves = position.generate::<BasicMoveList, AllGen, Legal>();
        assert!(!moves.is_empty());

        let flag = AtomicBool::new(true);
        let table = Table::new(1);
        let mut search = Search::new(position, &flag, None, &table);
        // Aborts are only honored once the guaranteed first ply has completed, which is the only
        // state in which an in-flight `quiesce_evasions` can be interrupted at runtime. Emulate
        // that armed state so the cancellation flag actually stops the search.
        search.min_search_complete = true;

        assert_eq!(
            search.quiesce_evasions::<Master, Pv>(Score::INF_N, Score::INF_P, &moves),
            None
        );
    }

    #[test]
    fn quiescence_uses_tt_scores_only_with_valid_bound_semantics() {
        core::init::init_globals();

        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);

        for (bound, stored, expected) in [
            (Bound::Exact, Score::cp(12), Score::cp(12)),
            (Bound::Lower, Score::cp(70), Score::cp(70)),
            (Bound::Upper, Score::cp(-70), Score::cp(-70)),
        ] {
            let table = Table::new(1);
            table
                .probe(&position)
                .into_inner()
                .write(&position, stored, 0, bound, &Move::null());
            let mut search = Search::new(position.clone(), &flag, None, &table);

            assert_eq!(
                search.quiesce::<Master, NonPv>(Score::cp(-50), Score::cp(-49)),
                Some(expected)
            );
        }
    }

    #[test]
    fn material_evaluation_scales_over_one_hundred_halfmoves() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 50 1").unwrap();
        let flag = AtomicBool::new(false);
        let tt = Table::new(1);
        let mut search = Search::new(pos, &flag, None, &tt);

        assert_eq!(search.evaluate(), Score::cp(450));
    }

    #[test]
    fn quiescence_does_not_use_a_search_score_as_static_evaluation() {
        core::init::init_globals();

        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        table.probe(&position).into_inner().write(
            &position,
            Score::cp(300),
            8,
            Bound::Exact,
            &Move::null(),
        );
        let mut search = Search::new(position, &flag, None, &table);

        assert_eq!(
            search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P),
            Some(Score::zero())
        );
    }

    #[test]
    fn quiescence_ignores_tt_slot_clashes() {
        core::init::init_globals();

        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let clashing_position = Position::from_fen("7k/8/8/8/8/8/8/K7 b - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(0);
        assert_eq!(table.capacity_entries(), 1);
        table.probe(&clashing_position).into_inner().write(
            &clashing_position,
            Score::cp(300),
            8,
            Bound::Exact,
            &Move::null(),
        );
        assert!(matches!(
            table.probe(&position),
            super::super::tt::Probe::Clash(_)
        ));
        let mut search = Search::new(position, &flag, None, &table);

        assert_eq!(
            search.quiesce::<Master, NonPv>(Score::cp(-1), Score::zero()),
            Some(Score::zero())
        );
    }

    /// A regression test to ensure that our search routine produces the expected results for a
    /// range of positions.
    #[test]
    fn gives_correct_answers() {
        core::init::init_globals();

        let suite = suite();

        for (fen, depth, lo, hi, bm) in suite {
            let pos = Position::from_fen(fen).unwrap();
            let flag = AtomicBool::new(false);
            let tt = Table::new(16);
            let mut search = Search::new(pos, &flag, None, &tt);
            let result = search.run::<Master>(depth).unwrap();

            assert!(lo <= result.score, "{fen}: {} < {lo}", result.score);
            assert!(result.score <= hi, "{fen}: {} > {hi}", result.score);
            assert_eq!(result.best_move.unwrap().to_uci_string(), bm, "{fen}");
        }
    }

    #[test]
    fn typed_api_returns_completed_search() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let search = engine.start(Position::start_pos(), SearchLimit::Depth(2));
        let outcome = search.wait();

        assert!(!outcome.was_cancelled());
        assert_eq!(outcome.result().unwrap().depth, 2);
        assert!(outcome.result().unwrap().best_move.is_some());
    }

    #[test]
    fn searches_reuse_the_shared_table_until_the_owner_clears_it() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let marker = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        assert_ne!(
            engine.table.idx(marker.zobrist().0),
            engine.table.idx(Position::start_pos().zobrist().0)
        );
        engine.table.probe(&marker).into_inner().write(
            &marker,
            Score::cp(17),
            1,
            Bound::Exact,
            &Move::null(),
        );

        engine
            .start(Position::start_pos(), SearchLimit::Depth(1))
            .wait();
        engine
            .start(Position::start_pos(), SearchLimit::Depth(1))
            .wait();
        assert!(engine.table.probe(&marker).is_hit());

        engine.clear_hash();
        assert!(!engine.table.probe(&marker).is_hit());
    }

    #[test]
    fn concurrent_searches_do_not_invalidate_the_shared_generation() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let marker = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        engine.table.probe(&marker).into_inner().write(
            &marker,
            Score::cp(17),
            1,
            Bound::Exact,
            &Move::null(),
        );

        let first = engine.start(Position::start_pos(), SearchLimit::Depth(2));
        let second = engine.start(Position::start_pos(), SearchLimit::Depth(2));
        first.wait();
        second.wait();

        assert!(engine.table.probe(&marker).is_hit());
    }

    #[test]
    fn typed_api_delivers_iterative_deepening_events() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let search = engine.start(Position::start_pos(), SearchLimit::Depth(2));
        let events = search.events().clone();
        let outcome = search.wait();
        let progress = events
            .try_iter()
            .filter_map(|event| match event {
                SearchEvent::Progress(progress) => Some(progress),
                SearchEvent::CurrentMove(_) => None,
            })
            .collect::<Vec<_>>();

        assert!(matches!(outcome, SearchOutcome::Completed(_)));
        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].depth, 1);
        assert_eq!(progress[1].depth, 2);
        assert!(progress.iter().all(|event| event.nodes > 0));
        assert!(progress
            .iter()
            .all(|event| !event.principal_variation.is_empty()));
    }

    /// FastChess reached this WAC-derived position after a long forcing line. At depth five the
    /// old search passed position-relative mate bounds to child nodes by negating them without
    /// first removing one ply. A cutoff value then leaked back as `Score::mate(34)`: positive with
    /// an impossible even ply count, and formatting the progress event tripped Score's parity
    /// assertion on the UCI driver thread.
    #[test]
    fn child_mate_windows_preserve_distance_parity() {
        core::init::init_globals();

        let position =
            Position::from_fen("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61").unwrap();
        let engine = SearchEngine::new(1);
        let search = engine.start(position, SearchLimit::Depth(5));
        let events = search.events().clone();
        let outcome = search.wait();
        let progress = events
            .try_iter()
            .filter_map(|event| match event {
                SearchEvent::Progress(progress) if progress.depth == 5 => Some(progress),
                _ => None,
            })
            .next()
            .expect("depth-five progress must be emitted");

        assert!(matches!(outcome, SearchOutcome::Completed(Some(_))));
        assert_eq!(progress.score, Score::mate(7));
        assert!(
            crate::info::format_search_event(&SearchEvent::Progress(progress))
                .contains("score mate 4")
        );
    }

    #[test]
    fn search_emits_typed_current_move_events() {
        core::init::init_globals();

        let mut position = Position::start_pos();
        let current_move = position.make_uci_move("e2e4").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let (sender, events) = unbounded();
        let search = Search::with_events(position, &flag, None, &table, sender);

        search.emit_current_move(7, &current_move, 4);

        assert_eq!(
            events.recv().unwrap(),
            SearchEvent::CurrentMove(CurrentMove {
                depth: 7,
                current_move,
                number: 4,
            })
        );
    }

    #[test]
    fn typed_api_cancels_running_search() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let search = engine.start(Position::start_pos(), SearchLimit::Infinite);
        let events = search.events().clone();
        search
            .events()
            .recv_timeout(Duration::from_secs(2))
            .expect("search should produce progress before cancellation");
        search.cancel();
        let outcome = search.wait();

        assert!(outcome.was_cancelled());
        assert!(outcome.result().unwrap().depth >= 1);
        assert!(outcome.result().unwrap().best_move.is_some());
        assert!(events.try_iter().all(|event| match event {
            SearchEvent::Progress(progress) => {
                progress.principal_variation.len() <= usize::from(progress.depth)
            }
            SearchEvent::CurrentMove(_) => true,
        }));
    }

    #[test]
    fn mid_subtree_abort_keeps_the_last_completed_iteration() {
        core::init::init_globals();

        let position = Position::start_pos();
        let flag = AtomicBool::new(false);

        // Measure the deterministic depth-one work, then stop a fresh search in the first child
        // of the candidate depth-two root. The root itself is the first new node and its child is
        // the second, so this threshold proves that a move was made and a subtree was entered.
        let baseline_table = Table::new(16);
        let mut baseline = Search::new(position.clone(), &flag, None, &baseline_table);
        let expected = baseline.run::<Master>(1).unwrap();
        let expected_pv = baseline.pvt.pv().copied().collect::<Vec<_>>();
        let completed_iteration_nodes = baseline.trace.all_nodes_visited();
        let abort_after = completed_iteration_nodes + 2;

        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);
        search.abort_after_nodes = Some(abort_after);
        let result = search.run::<Master>(3).unwrap();

        assert_eq!(result, expected);
        assert!(search.trace.all_nodes_visited() >= abort_after);
        assert_eq!(search.pvt.pv().copied().collect::<Vec<_>>(), expected_pv);

        // The aborted depth-two root must not replace the completed depth-one root entry.
        let root_entry = table.probe(&position).into_inner().read();
        assert_eq!(root_entry.depth, 1);
        assert_eq!(
            root_entry.mov.to_move(&position),
            expected.best_move.unwrap()
        );
    }

    #[test]
    fn aborted_child_cannot_score_or_write_its_parent() {
        core::init::init_globals();

        let position = Position::start_pos();
        let start_zob = position.zobrist();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);

        // Permit the test abort immediately and fire it in the first child: the root consumes one
        // node, makes a move, and the recursive search consumes the second node before stopping.
        search.min_search_complete = true;
        search.search_depth = 2;
        search.pvt = PVTable::new(2);
        search.abort_after_nodes = Some(2);

        let result = search.search::<Master, Root>(Score::INF_N, Score::INF_P, 2);

        assert_eq!(result, None, "an aborted child must not yield a score");
        assert_eq!(search.trace.all_nodes_visited(), 2);
        assert_eq!(search.pos.zobrist(), start_zob, "the root move is restored");
        assert!(
            search.pvt.pv().next().is_none(),
            "an aborted child must not become the principal move"
        );
        assert!(
            table.probe(&position).into_inner().read().is_empty(),
            "an ancestor whose child aborted must not write a TT entry"
        );
    }

    #[test]
    fn zero_time_limit_still_returns_a_legal_move() {
        core::init::init_globals();

        let position = Position::start_pos();
        let engine = SearchEngine::new(1);
        let search = engine.start(position.clone(), SearchLimit::Time(Duration::ZERO));
        let outcome = search.wait();

        // A zero budget must never forfeit: the guaranteed-minimum ply completes and yields a
        // legal move rather than an absent result (which UCI would emit as `bestmove 0000`).
        assert!(matches!(outcome, SearchOutcome::Completed(_)));
        let result = outcome.result().expect("a legal move must be returned");
        assert!(result.depth >= 1);
        let best_move = result
            .best_move
            .expect("non-terminal position has a legal move");
        assert!(
            position.valid_move(&best_move),
            "returned move must be legal"
        );
    }

    #[test]
    fn near_zero_time_budget_completes_the_guaranteed_ply() {
        core::init::init_globals();

        let position = Position::start_pos();
        let engine = SearchEngine::new(1);
        let search = engine.start(position.clone(), SearchLimit::Time(Duration::from_nanos(1)));
        let result = search.wait().result().cloned();

        let result = result.expect("near-zero budget must still return a legal move");
        assert!(result.depth >= 1);
        assert!(position.valid_move(&result.best_move.unwrap()));
    }

    #[test]
    fn aborts_are_suppressed_only_until_the_first_ply_completes() {
        core::init::init_globals();

        let position = Position::start_pos();
        // Both abort sources are active from the outset: the cancellation flag is set and the time
        // deadline has already elapsed.
        let flag = AtomicBool::new(true);
        let table = Table::new(1);
        let mut search = Search::new(position, &flag, Some(Instant::now()), &table);

        // Before the guaranteed ply completes, neither an elapsed deadline nor a set cancellation
        // flag may abort the search, so a legal root move is always produced.
        assert!(!search.stopping());

        // Once the guaranteed ply is complete the same signals are honored.
        search.min_search_complete = true;
        assert!(search.stopping());
    }

    #[test]
    fn time_limited_search_honors_the_budget_after_the_guaranteed_ply() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let search = engine.start(
            Position::start_pos(),
            SearchLimit::Time(Duration::from_millis(20)),
        );
        let outcome = search.wait();

        // The search returns of its own accord (the deadline aborts it) rather than running to the
        // maximum depth, and it still reports a completed legal move.
        assert!(matches!(outcome, SearchOutcome::Completed(_)));
        let result = outcome.result().expect("a legal move must be returned");
        assert!(result.depth >= 1);
        assert!(
            result.depth < MAX_DEPTH,
            "the budget must bound the search depth"
        );
    }

    #[test]
    fn immediate_cancellation_returns_an_explicit_optional_result() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let search = engine.start(Position::start_pos(), SearchLimit::Infinite);
        search.cancel();
        let outcome = search.wait();

        assert!(outcome.was_cancelled());
        assert!(outcome
            .result()
            .is_none_or(|result| result.best_move.is_some()));
    }

    #[test]
    fn terminal_position_returns_score_without_a_best_move() {
        core::init::init_globals();

        let position = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        let engine = SearchEngine::new(1);
        let outcome = engine.start(position, SearchLimit::Depth(1)).wait();
        let result = outcome.result().unwrap();

        assert!(matches!(outcome, SearchOutcome::Completed(Some(_))));
        assert_eq!(result.depth, 1);
        assert_eq!(result.best_move, None);
    }

    #[test]
    fn typed_api_supports_time_limits() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let search = engine.start(
            Position::start_pos(),
            SearchLimit::Time(Duration::from_millis(10)),
        );
        let outcome = search.wait();

        assert!(matches!(outcome, SearchOutcome::Completed(_)));
    }

    /// The self-play game, replayed verbatim from the FastChess record, whose final position made
    /// seaborg report `info depth 4 ... score mate -2 ... pv d7f8 g6a6 f8g6 c5f8` — a line whose
    /// fourth ply `c5f8` is illegal. The move list is used rather than the equivalent FEN because
    /// the repetition history it builds up is part of what the search sees.
    const ILLEGAL_MATE_PV_GAME: &str = "a2a3 a7a6 b2b3 a6a5 c2c3 b7b6 d2d3 b6b5 e2e3 a5a4 b3a4 \
        b5a4 f2f3 c7c6 g2g3 c6c5 h2h3 d7d6 c3c4 d6d5 c4d5 d8d5 d3d4 c5d4 e3d4 e7e6 g3g4 e6e5 d4e5 \
        d5a5 e1f2 a5e5 a1a2 f7f6 a2e2 f8c5 f2e1 e5e2 f1e2 a8a5 c1d2 a5a7 d1c2 a7b7 b1c3 e8d8 c3b5 \
        b8d7 d2a5 c5b6 c2a4 b6a5 a4a5 d8e7 f3f4 g7g6 g4g5 f6g5 a5c3 g8f6 f4g5 h7h6 c3e3 e7f8 e3c1 \
        f8e7 c1e3 e7f8 e3c3 f8e7 c3b4 e7e6 e2c4 e6e5 b4b2 e5f4 g5f6 h8e8 g1e2 f4g5 b2c1 g5h5 b5d6 \
        e8e5 a3a4 b7c7 d6f7 g6g5 h3h4 c8b7 h1h2 e5e4 h4g5 h5g6 h2h6 g6f5 f7d6 f5e5 d6e4 c7c4 c1c4 \
        b7e4 f6f7 e4g6 h6g6";

    /// Positions whose reported PVs are checked for legality: the pinned self-play reproduction,
    /// two opening positions, and the mate and tactical positions from the search suite, which are
    /// the mate-scored/shallow lines the defect surfaced on.
    fn pv_legality_positions() -> Vec<(String, Position)> {
        let mut positions = vec![(
            format!("startpos moves {ILLEGAL_MATE_PV_GAME}"),
            position_after(ILLEGAL_MATE_PV_GAME),
        )];

        for fen in ["rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"]
            .into_iter()
            .chain(suite().iter().map(|entry| entry.0))
        {
            positions.push((fen.to_owned(), Position::from_fen(fen).unwrap()));
        }

        positions
    }

    fn position_after(moves: &str) -> Position {
        let mut position = Position::start_pos();

        for uci in moves.split_whitespace() {
            position
                .make_uci_move(uci)
                .unwrap_or_else(|| panic!("{uci} should be legal in {}", position.to_fen()));
        }

        position
    }

    /// Replays a reported principal variation exactly as a UCI GUI would: each move must be legal
    /// in the position reached by playing the preceding PV moves.
    fn assert_pv_is_legal(label: &str, root: &Position, depth: u8, pv: &[Move]) {
        let mut position = root.clone();

        for (index, mov) in pv.iter().enumerate() {
            let uci = mov.to_uci_string();
            assert!(
                position.make_uci_move(&uci).is_some(),
                "illegal PV move at ply {} ({uci}) of depth-{depth} pv [{}] \
                 reported for `{label}`; illegal in {}",
                index + 1,
                pv.iter()
                    .map(|m| m.to_uci_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                position.to_fen(),
            );
        }
    }

    /// Collects every principal variation the search reports over the typed event channel.
    fn reported_pvs(engine: &SearchEngine, root: &Position, depth: u8) -> Vec<(u8, Vec<Move>)> {
        let search = engine.start(root.clone(), SearchLimit::Depth(depth));
        let events = search.events().clone();
        let _ = search.wait();

        events
            .try_iter()
            .filter_map(|event| match event {
                SearchEvent::Progress(progress) => {
                    Some((progress.depth, progress.principal_variation))
                }
                SearchEvent::CurrentMove(_) => None,
            })
            .collect()
    }

    /// Every move of every reported PV must be legal in the position it is played from. Regression
    /// for illegal deep PV plies spliced up from a stale sibling row or published by a fail-high
    /// node, which produced `pv d7f8 g6a6 f8g6 c5f8` scored `mate -2` in self-play.
    #[test]
    fn reported_principal_variations_are_legal() {
        core::init::init_globals();

        for (label, root) in pv_legality_positions() {
            // A fresh engine per position keeps the transposition table cold; the second pass
            // reuses the warm table, which is the state self-play actually reports from.
            let engine = SearchEngine::new(1);

            for _ in 0..2 {
                for depth in 1..=6 {
                    for (reported_depth, pv) in reported_pvs(&engine, &root, depth) {
                        assert_pv_is_legal(&label, &root, reported_depth, &pv);
                    }
                }
            }
        }
    }
}
