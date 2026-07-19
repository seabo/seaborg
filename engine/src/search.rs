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
    /// The ownership boundary is enforced rather than merely documented. [`Table::clear`] needs an
    /// exclusive reference, and `Arc::get_mut` only yields one once no worker holds a clone of the
    /// table — that is, once every search that could still be relying on its contents has finished.
    /// A caller that has not stopped its searches gets a panic here rather than silently pulling the
    /// table out from under a running worker.
    pub fn clear_hash(&mut self) {
        Arc::get_mut(&mut self.table)
            .expect("the hash cannot be cleared while a search still holds the table")
            .clear();
    }

    /// Begin a new game with an empty transposition table.
    ///
    /// Normal searches reuse the existing contents; only the session owner discards them.
    pub fn new_game(&mut self) {
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
        // Stamp entries written from now on with a fresh age, so that when this search competes for
        // a slot with results left by earlier ones, the earlier ones are the cheaper thing to give
        // up. Ages never invalidate: everything already in the table stays readable.
        self.table.advance_age();
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
            // has been observed to be lost, parking the driver forever.
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
    /// Cancel the worker and wait for it to exit.
    ///
    /// Joining rather than detaching is what makes "no search is running" a structural property
    /// instead of a caller convention. The worker holds a clone of the shared transposition table,
    /// and [`SearchEngine::clear_hash`] needs an exclusive reference to it, so a detached worker
    /// outliving its handle would make an otherwise correct `ucinewgame` panic — intermittently,
    /// and pointing at the clear rather than at the drop that caused it. Once every handle either
    /// joins through [`SearchHandle::wait`] or joins here, no path can leave a worker behind.
    ///
    /// The join always terminates: cancellation is checked on the search hot path, and neither
    /// channel the worker writes on its way out can block it (the events channel is unbounded, and
    /// the completion channel has capacity for the single message ever sent on it).
    ///
    /// The join result is discarded. There is no consumer for the outcome here, and a worker that
    /// panicked must not panic this thread in turn: during unwinding that would abort the process.
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            self.cancel();
            let _ = join.join();
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
    /// Counts every history-sensitive draw short-circuit taken during this search.
    ///
    /// A draw claimed by repetition or by the fifty-move rule is a property of how the position was
    /// reached, not of the position itself, so it is not covered by the Zobrist key. A node samples
    /// this counter before searching its children and compares it afterwards; if it moved, the
    /// node's value depends on the current history and must not be stored as position-intrinsic
    /// exact information. See `is_history_draw`.
    ///
    /// # Transposition-table reuse policy
    ///
    /// The Zobrist key covers pieces, side to move, castling rights and the en-passant file. It does
    /// not cover the halfmove clock or the move history, so a stored search value is only reusable
    /// where those uncovered parts of the state cannot change the answer. Three rules enforce that,
    /// and one known gap remains:
    ///
    /// 1. *Writes are suppressed for history-sensitive values.* A node whose subtree claimed a
    ///    repetition or fifty-move draw is not written at all (Step 24). Downgrading `Exact` to a
    ///    bound would not do: a draw score can raise a value to a beta cutoff as readily as it can
    ///    cap it, so the resulting bound is unsound in an incompatible history too. Consequently no
    ///    entry in the table embeds a draw that depends on how the position was reached.
    ///
    /// 2. *Reads are gated on the halfmove clock.* Because of rule 1, a stored value ignores the
    ///    fifty-move rule; `clock_permits_tt_reuse` therefore only allows a cutoff where the rule is
    ///    still out of reach within the stored depth.
    ///
    /// 3. *Leaf values are position-intrinsic.* `evaluate` does not read the clock, so the only
    ///    clock dependence left in a propagated score is the one rules 1 and 2 handle.
    ///
    /// # Known gap: repetition on the read side
    ///
    /// Rules 1 and 2 make a stored value independent of the history it was *computed* in. They do
    /// not make it valid in every history it is *read* in. A value computed where no descendant
    /// repeated can still be reused on a path where a descendant now repeats a position played
    /// before the root, and there the true value is a draw. This is the graph-history-interaction
    /// problem, and closing it needs entries keyed or gated by path history, which means reworking
    /// the table's layout, replacement policy and sizing. That is deliberately out of scope here;
    /// the engine accepts the resulting rare misvaluation, as mainstream engines do.
    ///
    /// Rule 1 applies to quiescence exactly as it does to the main search: `store_quiescence`
    /// carries the same comparison, so no writer of this table publishes a history-sensitive value.
    history_draws: u64,
    /// Flag to indicate when the search should start unwinding due to user intervention.
    stopping: &'engine AtomicBool,
    /// Time to at which to end search.
    stop_time: Option<Instant>,
    /// Node count at the most recent deadline sample. `usize::MAX` means that a sampled deadline
    /// expired and remains latched while the search unwinds. Only the comparatively expensive
    /// clock read is throttled; the cancellation flag is still read on every call.
    last_deadline_check_nodes: Option<usize>,
    /// Whether the guaranteed-minimum search (one full ply) has completed. The time deadline is
    /// suppressed until this is set, so a search always returns a completed legal root move even
    /// when the allotted budget is zero or already elapsed.
    min_search_complete: bool,
    /// Whether a legal root fallback has been established. The explicit cancellation flag is
    /// suppressed until this is set, and from then on it aborts immediately: the fallback
    /// guarantees a legal bestmove without waiting for the (unbounded) depth-1 quiescence tree.
    root_fallback_ready: bool,
    /// The move to report if cancellation ends the search before any iteration completes. It starts
    /// as the first generated legal root move and is upgraded to the best fully searched root move
    /// as the first ply progresses. `None` only for a terminal root position.
    root_fallback: Option<Move>,
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
            history_draws: 0,
            stopping: flag,
            stop_time,
            last_deadline_check_nodes: None,
            events,
            search_depth: 0,
            depth_reached: 0,
            min_search_complete: false,
            root_fallback_ready: false,
            root_fallback: None,
            #[cfg(test)]
            abort_after_nodes: None,
        }
    }

    pub fn run<T: Thread>(&mut self, d: u8) -> Option<SearchResult> {
        self.trace = Tracer::new();
        self.last_deadline_check_nodes = None;

        assert!(d > 0);

        // Some bookeeping and prep.
        let start_zob = self.pos.zobrist();

        self.trace.commence_search();
        self.search_depth = d;
        self.min_search_complete = false;
        self.root_fallback_ready = false;
        self.root_fallback = None;

        let result = self.iterative_deepening::<T>(d);
        self.trace.end_search();

        assert_eq!(start_zob, self.pos.zobrist());

        if let Some(result) = &result {
            self.report_telemetry(d, result.score);
        }

        self.history.reset();

        result
    }

    /// The statistics gathered by the most recent [`Search::run`].
    ///
    /// Elapsed time alone cannot explain a change in search speed. A search that finishes sooner
    /// because it visited fewer nodes got better informed; one that finishes sooner over the same
    /// nodes got cheaper per node. Node counts and probe outcomes separate the two, and unlike the
    /// timings they are exact and reproduce run to run, so a measurement harness needs them
    /// alongside the clock.
    pub fn trace(&self) -> &Tracer {
        &self.trace
    }

    fn iterative_deepening<T: Thread>(&mut self, depth: u8) -> Option<SearchResult> {
        let mut result = None;

        self.establish_root_fallback();

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

            // The first full ply is guaranteed to run against the clock; from here on the time-based
            // deadline is honored so deeper iterations respect the allotted clock.
            self.min_search_complete = true;
        }

        // Cancellation can end the search before any iteration completes. Report the fallback so
        // the position's legal move is still played; a terminal root has none, which UCI renders as
        // `bestmove 0000`. The score is not a search result and the depth records that no iteration
        // finished, so neither is reported as one.
        result.or_else(|| {
            self.root_fallback.map(|best_move| SearchResult {
                score: Score::zero(),
                best_move: Some(best_move),
                depth: 0,
            })
        })
    }

    /// Record a legal bestmove for the root position before any node is searched.
    ///
    /// Explicit cancellation is honored only once this has run. Root move generation is finite and
    /// cheap, so the window in which cancellation is ignored is bounded by move generation rather
    /// than by the depth-1 quiescence tree, which has no practically small bound.
    fn establish_root_fallback(&mut self) {
        self.root_fallback = self
            .pos
            .generate::<BasicMoveList, AllGen, Legal>()
            .first()
            .copied();
        self.root_fallback_ready = true;
    }

    /// Wraps [`Self::search_inner`] with the same node-score check quiescence carries, so the
    /// invariant is enforced wherever a score is produced rather than only in the subtree where
    /// the excursion was first observed. Root scores reach `Display` on the UCI thread, and an
    /// out-of-band one trips its parity assertion there.
    pub fn search<T: Thread, Node: NodeType>(
        &mut self,
        alpha: Score,
        beta: Score,
        depth: u8,
    ) -> NodeResult {
        let result = self.search_inner::<T, Node>(alpha, beta, depth);

        if let Some(score) = result {
            debug_assert!(
                score.is_node_score(),
                "search returned {score:?} outside the node score band \
                 (window {alpha:?}..{beta:?}, depth {depth})",
            );
        }

        result
    }

    fn search_inner<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        mut beta: Score,
        depth: u8,
    ) -> NodeResult {
        self.trace.visit_node();

        let draft = self.search_depth - depth;

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
        if self.is_history_draw() {
            return Some(Score::zero());
        }

        // Sampled before any child is searched, and compared again at the transposition-table write
        // below. If a history-sensitive draw was claimed anywhere in this subtree, `best_value`
        // depends on the path taken to reach this node and cannot be stored as exact information
        // about the position itself.
        let history_draws_on_entry = self.history_draws;

        // Normalize search bounds into the range a node can return.
        if !Node::root() {
            // This is deliberately not mate-distance pruning. Mate scores are position-relative,
            // so the root ply does not tighten a descendant's attainable mate range: every node
            // can still be checkmated now or mate on its next ply. The old root-relative `draft`
            // bounds were therefore unsound, and no equivalent pruning remains.
            //
            // The clamp is still required as representation hygiene. `child_bound` is exact, so a
            // window at the very bottom of the band arrives here as
            // `(Score(20_100), Score(20_101))`: entirely above anything a node can score. Clamping
            // both ends also maps the infinity bounds used at the root into the node-score band.
            // Neither operation discards an attainable score; it only prevents a threshold from
            // escaping as a fail-soft return value.
            alpha = alpha.clamp(Score::mate(0), Score::mate(1));
            beta = beta.clamp(Score::mate(0), Score::mate(1));
            // An exact child-bound conversion can put the whole window above or below the node
            // band. Normalization then collapses it. Returning the in-band threshold is required
            // before another recursive call, whose window must be non-empty; this is bound
            // sanitation, not a mate-distance cutoff.
            if alpha >= beta {
                return Some(alpha);
            }
        }

        // Step 3. Load transposition table entry.
        //
        // The probe returns an owned snapshot, so everything below reads one atomic state of one
        // slot. A concurrent worker replacing that slot between here and Step 24 cannot change what
        // this node consumes.
        //
        // Two independent things are extracted from a hit, and neither implies the other:
        //
        // * The *score*, which is reusable whenever the entry is deep enough and the clock permits.
        // * The *move*, which is only useful if it can actually be played here.
        //
        // Coupling them costs cutoffs for no safety. A checkmated or stalemated node stores its
        // value with no move at all, and so does every fail-low node whose moves all failed to
        // raise alpha; requiring a move before trusting the score makes exactly those entries — the
        // cheapest and most certain ones in the table — permanently unusable.
        //
        // Trusting the score without a move is safe because the entry's identity is already
        // established: `Table::probe` verifies the full 64-bit key against the same write the score
        // was decoded from, so accepting a foreign position's entry requires a genuine Zobrist
        // collision. Move legality is not part of that proof and never was — it filters some wrong
        // entries by accident, but says nothing about a move-less one. See the `tt` module docs.
        let tt_entry = self.tt.probe(self.pos.zobrist().0);
        let mut tt_mov = None;
        match tt_entry.as_ref() {
            Some(entry) => {
                self.trace.hash_hit();
                if let Some(packed) = entry.mov() {
                    let mov = packed.to_move(&self.pos);
                    if self.pos.valid_move(&mov) {
                        tt_mov = Some(mov);
                    } else {
                        // A verified entry whose move cannot be played here. Since the full key
                        // matched, this is a genuine Zobrist collision, and the counter measures
                        // that rather than a truncated-signature accident. The score is left alone:
                        // an unusable ordering hint is not evidence about the score's provenance.
                        self.trace.hash_collision();
                    }
                }
            }
            None => self.trace.hash_miss(),
        }

        // Step 4. Check for early cutoff.
        if !Node::pv() {
            if let Some(entry) =
                tt_entry.filter(|e| e.depth() >= depth && self.clock_permits_tt_reuse(e.depth()))
            {
                match entry.bound() {
                    Bound::Exact => {
                        return Some(entry.score());
                    }
                    Bound::Lower => {
                        if entry.score() > beta {
                            return Some(entry.score());
                        } else if entry.score() > alpha {
                            alpha = entry.score()
                        }
                    }
                    Bound::Upper => {
                        if entry.score() < alpha {
                            return Some(entry.score());
                        } else if entry.score() < beta {
                            beta = entry.score()
                        }
                    }
                }
            }

            if alpha >= beta {
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

                // The child's first act is to probe this cluster, and the table is far larger than
                // cache, so that probe misses. Starting the fetch here overlaps the miss with the
                // recursive descent's own setup rather than stalling on it. The key is only known
                // once the move has been made, so this is the earliest point the address exists.
                self.tt.prefetch(self.pos.zobrist().0);

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

                // Upgrade the cancellation fallback to the best fully searched root move, so a
                // cancellation during the first ply reports a searched move rather than the
                // arbitrary first generated one. An abort during this move's subtree leaves `value`
                // meaningless, so only a move searched without stopping may be adopted.
                if Node::root() && value > best_value && !self.stopping() {
                    self.root_fallback = Some(*mov);
                }

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
        //
        // A subtree that claimed a draw by repetition or by the fifty-move rule produced a value
        // that depends on the moves played before the root, which the Zobrist key does not cover.
        // Storing it would let a later visit with a different history reuse a draw that does not
        // apply there. Neither is it enough to downgrade `Exact` to a bound: a draw score can raise
        // the value to a beta cutoff just as easily as it can cap it, so the resulting `Lower` or
        // `Upper` bound is unsound in an incompatible history too. The entry is therefore left
        // unwritten and the position is re-searched when it is next reached.
        //
        // Reaching here also requires `stopping()` to have been false just above, so an entry can
        // only be published by a node whose whole move loop ran to completion. An aborted subtree
        // returns `None` before this point, and every child search propagates that `None` upwards,
        // so no partially explored value ever reaches the table.
        //
        // `depth` is at least one here: a depth-zero node delegated to quiescence at Step 5. That is
        // what reserves [`Self::QUIESCENCE_DRAFT`] for quiescence alone.
        debug_assert!(depth > Self::QUIESCENCE_DRAFT);
        if self.history_draws == history_draws_on_entry {
            self.tt.store(
                self.pos.zobrist().0,
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
        }

        // Step 25. Return best value.
        Some(best_value)
    }

    #[inline(always)]
    fn stopping(&mut self) -> bool {
        #[cfg(test)]
        if self
            .abort_after_nodes
            .is_some_and(|limit| self.trace.all_nodes_visited() >= limit)
        {
            return true;
        }

        // The two abort signals are gated separately.
        //
        // Explicit cancellation (`stop`, `quit`, stdin EOF, or a command replacing the active
        // search) aborts as soon as the root fallback exists, which is before the first node is
        // searched. A legal bestmove is therefore always available without waiting for the depth-1
        // quiescence tree, whose size has no practically small bound. This check reads an
        // atomic bool, which is cheap enough to run on every call and must stay unthrottled so that
        // cancellation responsiveness is unaffected.
        //
        // The time deadline is still suppressed until the guaranteed-minimum search (the first full
        // ply) completes, so a zero or already-elapsed budget returns a searched move rather than
        // the unsearched fallback. The first ply is finite, so this can never hang.
        if self.stopping.load(Ordering::Relaxed) {
            return self.root_fallback_ready;
        }

        if !self.min_search_complete {
            return false;
        }

        let Some(stop_time) = self.stop_time else {
            return false;
        };

        // Unlike the cancellation flag, the deadline needs a clock read, which is expensive enough
        // relative to a node to matter in the innermost loops. Optimized searches therefore sample
        // every eight nodes. Debug builds search orders of magnitude more slowly, so sample each
        // node there to keep wall-clock tests and developer runs responsive while still avoiding
        // repeated reads within the same node.
        const DEADLINE_CHECK_INTERVAL_NODES: usize = if cfg!(debug_assertions) { 1 } else { 8 };
        let nodes = self.trace.all_nodes_visited();
        if let Some(last) = self.last_deadline_check_nodes {
            // An expired deadline stays latched: the many stopping checks made while the search
            // unwinds must all agree, rather than the throttle letting search resume mid-unwind.
            if last == usize::MAX {
                return true;
            }
            if nodes.saturating_sub(last) < DEADLINE_CHECK_INTERVAL_NODES {
                return false;
            }
        }

        if stop_time <= Instant::now() {
            self.last_deadline_check_nodes = Some(usize::MAX);
            true
        } else {
            self.last_deadline_check_nodes = Some(nodes);
            false
        }
    }

    /// Reports whether the current position is an immediate draw by repetition or by the fifty-move
    /// rule, recording the claim so that ancestors can tell their value depends on this history.
    ///
    /// Both conditions read `Position::history`, which the Zobrist key does not cover: the same key
    /// is a draw in one line and a live position in another. Every caller must go through here so
    /// that the claim is counted, and so that both the main search and quiescence agree on the
    /// fifty-move boundary.
    #[inline(always)]
    fn is_history_draw(&mut self) -> bool {
        if self.pos.in_threefold() || self.pos.fifty_move_rule_reached() {
            self.history_draws += 1;
            true
        } else {
            false
        }
    }

    /// Plies a subtree may be searched beyond its nominal depth, through quiescence and check
    /// extensions. Used only to keep [`Self::clock_permits_tt_reuse`] on the conservative side of
    /// the fifty-move boundary.
    const HORIZON_SLACK: u32 = 16;

    /// The draft recorded for a value produced by quiescence.
    ///
    /// Quiescence and the main search share one table, so a reader has to be able to tell a
    /// capture-only value apart from a real depth-`d` search of the same position. The whole scheme
    /// rests on one reserved level: quiescence writes this draft and nothing else, and the main
    /// search never writes it, because a main-search node at depth zero delegates to quiescence
    /// before it can reach its own store. Every main-search entry therefore has a draft of at least
    /// one.
    ///
    /// That makes the ordinary `entry.depth() >= depth` test do the separation for free. A
    /// main-search node needs at least depth one, which no quiescence entry can satisfy, so a
    /// capture-only value can never masquerade as a searched one. A quiescence node needs nothing
    /// beyond this draft, so it can reuse its own results and any deeper main-search result. The
    /// only nodes that consume a quiescence entry are the ones whose own search *is* quiescence.
    const QUIESCENCE_DRAFT: u8 = 0;

    /// Reports whether a stored result of the given depth may be reused at this node, as far as the
    /// halfmove clock is concerned.
    ///
    /// Making the static evaluation position-intrinsic is not on its own enough to make reuse
    /// sound. A search value also reflects any fifty-move draw reachable inside its own subtree,
    /// and whether one is reachable depends on the clock, which the Zobrist key does not cover. The
    /// write side of this contract is enforced at Step 24: a node whose subtree claimed a fifty-move
    /// or repetition draw is never written, so a stored value never embeds such a draw. This is the
    /// matching read side: a value computed where the rule was out of reach must only be reused
    /// where it is still out of reach, or a drawn line is scored as if it played on.
    ///
    /// The horizon is bounded by the stored depth plus [`Self::HORIZON_SLACK`], since quiescence
    /// and check extensions can search past the nominal depth. That slack is a conservative
    /// allowance rather than a proof: quiescence follows captures, which reset the clock, and quiet
    /// check evasions, which do not, and the length of a forcing evasion sequence has no tight
    /// static bound. Erring high costs only hit rate, and only near the boundary.
    #[inline(always)]
    fn clock_permits_tt_reuse(&self, entry_depth: u8) -> bool {
        self.pos.half_move_clock() + entry_depth as u32 + Self::HORIZON_SLACK
            < Position::FIFTY_MOVE_RULE_PLIES
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    ///
    /// This is deliberately *position-intrinsic*: it depends only on state that the Zobrist key
    /// covers, and in particular not on the halfmove clock. The key identifies pieces, side to
    /// move, castling rights and the en-passant file, so the value returned here is the same at
    /// every visit to a position with the same key, whatever the clock reads there.
    ///
    /// This evaluation previously scaled material towards zero as the halfmove clock approached
    /// the fifty-move threshold. That made every propagated score a function of a value the key
    /// does not cover, so a warm table could return a score computed under a materially different
    /// clock. The approach of a fifty-move draw is instead left to the draw detection in `search`
    /// and `quiesce`, which the search discovers within its own horizon.
    ///
    /// Note that this makes the *leaf* value clock-independent, which is necessary for sound
    /// transposition-table reuse but not sufficient: a propagated value still reflects any
    /// fifty-move draw reachable inside its own subtree. That residual dependence is what
    /// [`Self::clock_permits_tt_reuse`] and the write suppression at Step 24 exist to contain.
    #[inline(always)]
    fn evaluate(&mut self) -> Score {
        Score::cp(self.pos.material_eval() * self.pov())
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
    ///
    /// Wraps [`Self::quiesce_inner`] so that every exit from quiescence passes one check: the
    /// score must lie in the band a node can actually hold. Quiescence returns `alpha` and `beta`
    /// directly as fail-soft scores, so a window bound that escaped the encoding would become a
    /// node score, and `Debug`/`Display` would render it as nonsense or trip their parity
    /// assertions. See [`Score::is_node_score`].
    fn quiesce<T: Thread, Node: NodeType>(&mut self, alpha: Score, beta: Score) -> NodeResult {
        let result = self.quiesce_inner::<T, Node>(alpha, beta);

        if let Some(score) = result {
            debug_assert!(
                score.is_node_score(),
                "quiescence returned {score:?} outside the node score band \
                 (window {alpha:?}..{beta:?})",
            );
        }

        result
    }

    fn quiesce_inner<T: Thread, Node: NodeType>(
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
        //
        // This must use the same boundary as the main search: the fifty-move rule counts 100 plies,
        // not 50. Comparing the clock against 50 here reported a draw at 25 moves.
        if self.is_history_draw() {
            return Some(Score::zero());
        }

        // Normalize search bounds into the range a node can return, on the same terms as `search`.
        //
        // This is not mate-distance pruning. Quiescence once had no equivalent normalization,
        // which let the bound excursion compound:
        // `child_bound` is exact, so `Score(20_101)` became the next ply's alpha, then
        // `Score(-20_102)`, and so on. Quiescence returns `alpha` and `beta` directly as fail-soft
        // scores, so those out-of-band bounds became node scores.
        alpha = alpha.clamp(Score::mate(0), Score::mate(1));
        beta = beta.clamp(Score::mate(0), Score::mate(1));
        if alpha >= beta {
            return Some(alpha);
        }

        // The window this node was given, kept for classifying whatever value it ends up storing.
        // Nothing below is allowed to move it, which is why the cutoff at Step 4 does not narrow the
        // live window: a bound recorded against a window a previous search supplied would describe
        // that search's result rather than this node's.
        let alpha_on_entry = alpha;

        // Sampled after the draw check above, on the same terms as the main search: if a
        // history-sensitive draw is claimed anywhere below this node, its value depends on how the
        // position was reached and must not be published as position-intrinsic. See
        // `Search::history_draws`.
        let history_draws_on_entry = self.history_draws;

        // Step 3. Load transposition table entry.
        let tt_entry = self.tt.probe(self.pos.zobrist().0);
        match tt_entry {
            Some(_) => self.trace.hash_hit(),
            None => self.trace.hash_miss(),
        }

        // Step 4. Check for early TT cutoff.
        if !Node::pv() {
            // A quiescence node searches to [`Self::QUIESCENCE_DRAFT`], so every entry in the table
            // is deep enough for it: its own earlier results, and any main-search result, which is
            // strictly better informed. The stored score remains an alpha-beta bound; it is never a
            // replacement for the position's static evaluation.
            //
            // Any verified entry may be trusted, with or without a move, for the reason set out in
            // the main search's Step 3: identity is established by the full-key check inside
            // `Table::probe`, not by whether the stored move happens to be playable here. The two
            // searches deliberately behave the same way, and quiescence not needing the move for
            // ordering is why it never looks at one.
            //
            // The clock gate applies here for the same reason it applies in the main search: a
            // stored value never accounts for the fifty-move rule, so it may only be reused where
            // the rule is still out of reach.
            if let Some(entry) = tt_entry.filter(|e| self.clock_permits_tt_reuse(e.depth())) {
                match entry.bound() {
                    Bound::Exact => {
                        return Some(entry.score());
                    }
                    Bound::Lower => {
                        if entry.score() >= beta {
                            return Some(entry.score());
                        }
                    }
                    Bound::Upper => {
                        if entry.score() <= alpha {
                            return Some(entry.score());
                        }
                    }
                }
            }
        }

        let in_check = self.pos.in_check();

        // Step 5. Static evaluation. Stand pat is not a legal option while in check.
        if !in_check {
            let stand_pat = self.evaluate();

            if stand_pat >= beta {
                // The value returned is the hard-fail `beta`, but what is *known* is the stronger
                // statement that this node is worth at least `stand_pat`. Recording the stronger
                // bound lets a later visit with a higher beta still cut off here.
                self.store_quiescence(
                    stand_pat,
                    Bound::Lower,
                    &Move::null(),
                    history_draws_on_entry,
                );
                return Some(beta);
            }

            if alpha < stand_pat {
                alpha = stand_pat;
            }
        }

        if in_check {
            let moves = self.pos.generate::<BasicMoveList, AllGen, Legal>();
            return self.quiesce_evasions::<T, Node>(alpha, beta, &moves, history_draws_on_entry);
        }

        // Step 6. Loop through all the moves until no moves remain or a beta cutoff occurs.
        let mut best_move = Move::null();
        let mut moves = OrderedMoves::new();
        'move_loop: while moves.load_next_phase(QMoveLoader::from(self)) {
            for mov in &moves {
                if self.stopping() {
                    break 'move_loop;
                }

                // SAFETY: quiescence moves originate from move generation for `self.pos`.
                unsafe { self.pos.make_move_unchecked(mov) };
                // As in the main search: start the child's cluster fetch as soon as its key exists,
                // so the miss overlaps the descent instead of stalling in front of the probe.
                self.tt.prefetch(self.pos.zobrist().0);
                let child = self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound());
                self.pos.unmake_move();
                // An aborted child leaves no usable value, and returning here without storing is
                // what keeps a truncated subtree out of the table.
                let score = child?.neg().inc_mate();

                if score >= beta {
                    self.store_quiescence(score, Bound::Lower, mov, history_draws_on_entry);
                    return Some(beta);
                }

                if score > alpha {
                    alpha = score;
                    best_move = *mov;
                }
            }
        }

        // A stop breaks out of the loop with some captures unexamined, so `alpha` describes a
        // subtree that was never finished. It is neither returned nor stored.
        if self.stopping() {
            return None;
        }

        self.store_quiescence(
            alpha,
            self.quiescence_bound(alpha, alpha_on_entry),
            &best_move,
            history_draws_on_entry,
        );
        Some(alpha)
    }

    fn quiesce_evasions<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        beta: Score,
        moves: &BasicMoveList,
        history_draws_on_entry: u64,
    ) -> NodeResult {
        // In check there is no stand pat, so the caller's alpha reaches here untouched and is still
        // the window this node was given.
        let alpha_on_entry = alpha;

        if moves.is_empty() {
            // Checkmate: terminal, certain, and with no continuation to record. This is the entry
            // shape that a move-gated cutoff can never reuse, which is why the cutoff paths in both
            // searches are gated on the score alone.
            self.store_quiescence(
                Score::mate(0),
                Bound::Exact,
                &Move::null(),
                history_draws_on_entry,
            );
            return Some(Score::mate(0));
        }

        let mut best_move = Move::null();

        for mov in moves {
            if self.stopping() {
                return None;
            }

            self.pos.make_move(mov);
            let child = self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound());
            self.pos.unmake_move();
            let score = child?.neg().inc_mate();

            if score >= beta {
                self.store_quiescence(score, Bound::Lower, mov, history_draws_on_entry);
                return Some(beta);
            }

            if score > alpha {
                alpha = score;
                best_move = *mov;
            }
        }

        self.store_quiescence(
            alpha,
            self.quiescence_bound(alpha, alpha_on_entry),
            &best_move,
            history_draws_on_entry,
        );
        Some(alpha)
    }

    /// Classifies a quiescence value that neither reached beta nor was cut short.
    ///
    /// Quiescence fails hard, so the value it returns is `alpha`, and what that means depends
    /// entirely on whether anything raised it. A raised alpha was produced by a child that scored
    /// strictly inside its own window, or by a stand pat that no capture beat; either way it is the
    /// position's quiescence value rather than a threshold, so it is exact. An alpha that never
    /// moved carries no information beyond "nothing here reached it", which is an upper bound.
    #[inline(always)]
    fn quiescence_bound(&self, alpha: Score, alpha_on_entry: Score) -> Bound {
        if alpha > alpha_on_entry {
            Bound::Exact
        } else {
            Bound::Upper
        }
    }

    /// Publishes a completed quiescence result at [`Self::QUIESCENCE_DRAFT`].
    ///
    /// Every caller has already established that the value came from work that ran to completion:
    /// an aborted quiescence subtree propagates `None` and never arrives here. The remaining
    /// condition is the one the main search applies at Step 24 — a value that a history-sensitive
    /// draw contributed to is not a property of the position, so it is dropped rather than stored.
    #[inline]
    fn store_quiescence(
        &self,
        score: Score,
        bound: Bound,
        mov: &Move,
        history_draws_on_entry: u64,
    ) {
        if self.history_draws == history_draws_on_entry {
            self.tt.store(
                self.pos.zobrist().0,
                score,
                Self::QUIESCENCE_DRAFT,
                bound,
                mov,
            );
        }
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
                " misses:     {:>8} ({:.1}%)",
                self.trace.hash_misses().separated_string(),
                self.trace.hash_misses() as f64 / self.trace.hash_probes() as f64 * 100.
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
    use core::mov::MoveType;
    use core::position::Square;
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

        // Losing the rook is worth exactly its material value. This previously read -495: the
        // evasion search advanced the halfmove clock to 1, and the old clock-scaled evaluation
        // shaded the score by a percent for it. Evaluation is now position-intrinsic.
        assert_eq!(score, Some(Score::cp(-500)));
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
        // Cancellation is only honored once a legal root fallback exists, which `run` establishes
        // before any node is searched. Emulate that armed state so the flag actually stops the
        // search.
        search.root_fallback_ready = true;

        assert_eq!(
            search.quiesce_evasions::<Master, Pv>(Score::INF_N, Score::INF_P, &moves, 0),
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
            table.store(position.zobrist().0, stored, 0, bound, &Move::null());
            let mut search = Search::new(position.clone(), &flag, None, &table);

            assert_eq!(
                search.quiesce::<Master, NonPv>(Score::cp(-50), Score::cp(-49)),
                Some(expected)
            );
        }
    }

    /// The evaluation must not depend on the halfmove clock, which the Zobrist key does not
    /// cover. It previously scaled material towards zero as the clock advanced, so this position
    /// evaluated to 450 at a clock of 50 and 900 at a clock of 0.
    #[test]
    fn material_evaluation_is_independent_of_the_halfmove_clock() {
        core::init::init_globals();

        for halfmove_clock in [0, 50, 99] {
            let fen = format!("4k3/8/8/8/8/8/8/Q3K3 w - - {halfmove_clock} 1");
            let pos = Position::from_fen(&fen).unwrap();
            let flag = AtomicBool::new(false);
            let tt = Table::new(1);
            let mut search = Search::new(pos, &flag, None, &tt);

            assert_eq!(
                search.evaluate(),
                Score::cp(900),
                "evaluation moved at halfmove clock {halfmove_clock}"
            );
        }
    }

    /// A table warmed at one halfmove clock must return the same score when the identical
    /// position is searched at a materially different clock. Before evaluation became
    /// position-intrinsic the warm result was computed under the warming clock and silently reused.
    #[test]
    fn warm_table_reuse_agrees_across_materially_different_halfmove_clocks() {
        core::init::init_globals();

        let score_at = |halfmove_clock: u32, table: &Table| {
            let fen = format!("4k3/8/8/8/8/5N2/8/Q3K3 w - - {halfmove_clock} 1");
            let pos = Position::from_fen(&fen).unwrap();
            let flag = AtomicBool::new(false);
            let mut search = Search::new(pos, &flag, None, table);
            search.run::<Master>(4).unwrap().score
        };

        // Warm the table at a low clock, then search the same position at a high one. A cold
        // reference search at the high clock must agree with the warm result.
        let warm_table = Table::new(16);
        let _ = score_at(0, &warm_table);
        let warm = score_at(80, &warm_table);

        let cold = score_at(80, &Table::new(16));

        assert_eq!(
            warm, cold,
            "a warm table changed the score at a different halfmove clock"
        );
    }

    /// Position-intrinsic evaluation is not on its own enough. A stored value never accounts for
    /// the fifty-move rule, because a node whose subtree claimed the draw is not written at all.
    /// Reusing such a value where the boundary *is* within reach scores a dead-drawn line as if it
    /// played on, so the read side must refuse the cutoff.
    ///
    /// The position below establishes the premise that gate exists for: one key, two materially
    /// different true values, told apart only by the halfmove clock. White is a queen and a knight
    /// up against a bare king. At a fresh clock that is worth the full 1200. At clock 96 a four-ply
    /// search sees that every quiet continuation runs the clock to 100 and draws, so its best line
    /// is to hang the queen: the king's capture resets the clock and keeps the knight, worth 300.
    /// Reusing the 1200 where the 300 applies is the defect.
    #[test]
    fn the_same_key_is_worth_different_scores_at_different_halfmove_clocks() {
        core::init::init_globals();

        let score_at = |halfmove_clock: u32| {
            let fen = format!("4k3/8/8/8/8/5N2/8/Q3K3 w - - {halfmove_clock} 1");
            let pos = Position::from_fen(&fen).unwrap();
            let flag = AtomicBool::new(false);
            let table = Table::new(16);
            let mut search = Search::new(pos, &flag, None, &table);
            search.run::<Master>(4).unwrap().score
        };

        assert_eq!(
            score_at(0),
            Score::cp(1200),
            "material is intact at a fresh clock"
        );
        assert_eq!(
            score_at(96),
            Score::cp(300),
            "near the boundary the queen must be given up to reset the clock"
        );
    }

    /// The main search must refuse a stored cutoff once the fifty-move boundary is
    /// within the stored entry's reach.
    ///
    /// Seeding the entry directly, rather than warming the table with a real search, is deliberate:
    /// it pins the cutoff path under test instead of depending on which keys a warming search
    /// happens to leave behind and at what depth. The previous revision's warm-table test asserted
    /// only that two searches agreed, which held whether or not the gate was present.
    #[test]
    fn the_main_search_refuses_a_stored_cutoff_near_the_fifty_move_boundary() {
        core::init::init_globals();

        // Bare kings: the true value is 0, so a seeded 300 can only come from the table.
        let seeded_score = Score::cp(300);
        let seeded_depth = 8;

        let score_at = |halfmove_clock: u32| {
            let fen = format!("7k/8/8/8/8/8/8/K7 w - - {halfmove_clock} 1");
            let position = Position::from_fen(&fen).unwrap();
            let flag = AtomicBool::new(false);
            let table = Table::new(1);

            // Step 4 only takes a cutoff when the entry also carries a usable move.
            let moves = position.generate::<BasicMoveList, AllGen, Legal>();
            let mov = *moves
                .iter()
                .find(|m| format!("{m}").contains("a1b2"))
                .expect("the king move must be legal");
            table.store(
                position.zobrist().0,
                seeded_score,
                seeded_depth,
                Bound::Exact,
                &mov,
            );

            let mut search = Search::new(position, &flag, None, &table);
            search.search_depth = 4;
            search.pvt = PVTable::new(4);
            search.search::<Master, NonPv>(Score::cp(299), Score::cp(300), 4)
        };

        assert_eq!(
            score_at(0),
            Some(seeded_score),
            "well below the boundary the stored cutoff must still be taken"
        );

        // 90 + 8 + 16 exceeds 100, so the rule is within the entry's reach and the value it was
        // computed under no longer applies. With the cutoff refused the position is searched, the
        // true value of 0 is far below the window, and the null-window search fails low on alpha.
        assert_eq!(
            score_at(90),
            Some(Score::cp(299)),
            "a stored value was reused where the fifty-move rule is within its reach"
        );
    }

    /// Quiescence probes the same table and needs the same gate.
    #[test]
    fn quiescence_refuses_a_stored_cutoff_near_the_fifty_move_boundary() {
        core::init::init_globals();

        let score_at = |halfmove_clock: u32| {
            let fen = format!("7k/8/8/8/8/8/8/K7 w - - {halfmove_clock} 1");
            let position = Position::from_fen(&fen).unwrap();
            let flag = AtomicBool::new(false);
            let table = Table::new(1);
            table.store(
                position.zobrist().0,
                Score::cp(300),
                8,
                Bound::Exact,
                &Move::null(),
            );

            let mut search = Search::new(position, &flag, None, &table);
            search.quiesce::<Master, NonPv>(Score::cp(299), Score::cp(300))
        };

        assert_eq!(score_at(0), Some(Score::cp(300)));

        // As above, refusing the cutoff leaves a stand-pat of 0 below the window, so quiescence
        // fails low on alpha rather than returning the seeded score.
        assert_eq!(
            score_at(90),
            Some(Score::cp(299)),
            "quiescence reused a stored value across the fifty-move boundary"
        );
    }

    /// The clock gate must be a boundary condition, not a blanket disable: reuse has to remain
    /// available at the clocks a search actually spends most of its time at.
    #[test]
    fn the_clock_gate_only_bites_near_the_fifty_move_boundary() {
        core::init::init_globals();

        let permits = |halfmove_clock: u32, entry_depth: u8| {
            let fen = format!("4k3/8/8/8/8/5N2/8/Q3K3 w - - {halfmove_clock} 1");
            let pos = Position::from_fen(&fen).unwrap();
            let flag = AtomicBool::new(false);
            let tt = Table::new(1);
            let search = Search::new(pos, &flag, None, &tt);
            search.clock_permits_tt_reuse(entry_depth)
        };

        assert!(permits(0, 8), "reuse must be available at a fresh clock");
        assert!(permits(60, 8), "reuse must survive an ordinary quiet phase");

        // 83 + 8 + 16 slack reaches exactly 100, the fifty-move boundary.
        assert!(!permits(83, 8), "reuse must stop at the boundary");
        assert!(!permits(96, 4), "reuse must stop close to the boundary");

        // Deeper entries reach further, so they must be cut off sooner.
        assert!(permits(60, 8) && !permits(60, 24));
    }

    /// Plays a four-ply king shuffle, returning a position whose history already contains one
    /// earlier occurrence of itself. A search from here can reach the threefold below the root.
    fn position_repeated_once() -> Position {
        let mut pos = Position::from_fen("6k1/8/8/8/8/8/8/1K6 w - - 0 1").unwrap();

        for san in ["b1a1", "g8h8", "a1b1", "h8g8"] {
            let moves = pos.generate::<BasicMoveList, AllGen, Legal>();
            let mov = *moves
                .iter()
                .find(|m| format!("{m}").contains(san))
                .expect("shuffle move must be legal");
            pos.make_move(&mov);
        }

        pos
    }

    /// A value produced by a repetition claim depends on the moves played before the root,
    /// which the key does not cover, so it must not reach the table at all.
    #[test]
    fn a_repetition_derived_value_is_not_stored_in_the_table() {
        core::init::init_globals();

        let position = position_repeated_once();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);

        // Driven at a single fixed depth rather than through iterative deepening. Four plies are
        // needed to reach the third occurrence, so a deepening search would first store legitimate
        // history-independent results from its shallower iterations and mask the suppression.
        search.search_depth = 4;
        search.pvt = PVTable::new(4);
        search
            .search::<Master, Root>(Score::INF_N, Score::INF_P, 4)
            .unwrap();

        assert!(
            search.history_draws > 0,
            "the test position must actually exercise a repetition claim"
        );
        assert!(
            table.probe(position.zobrist().0).is_none(),
            "a repetition-contaminated value must not be written to the table"
        );
    }

    /// The same holds for the fifty-move rule, the other draw the key does not cover: a
    /// subtree that crosses the boundary produces a value that only applies at this clock.
    #[test]
    fn a_fifty_move_derived_value_is_not_stored_in_the_table() {
        core::init::init_globals();

        // Two plies into the search the clock reaches 100 and the draw is claimed below the root.
        let position = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 98 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);

        search.search_depth = 3;
        search.pvt = PVTable::new(3);
        search
            .search::<Master, Root>(Score::INF_N, Score::INF_P, 3)
            .unwrap();

        assert!(
            search.history_draws > 0,
            "the test position must actually cross the fifty-move boundary"
        );
        assert!(
            table.probe(position.zobrist().0).is_none(),
            "a clock-contaminated value must not be written to the table"
        );
    }

    /// A position whose subtree never claimed a history-sensitive draw is ordinary
    /// position-intrinsic information and must still be stored, so the policy above is not a
    /// blanket suppression of the table.
    #[test]
    fn a_history_independent_value_is_still_stored_in_the_table() {
        core::init::init_globals();

        let position = Position::from_fen("4k3/8/8/8/8/5N2/8/Q3K3 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);

        let draws_before = search.history_draws;
        search.run::<Master>(3).unwrap();

        assert_eq!(
            search.history_draws, draws_before,
            "this position must not claim a history-sensitive draw"
        );
        assert!(
            !table.probe(position.zobrist().0).is_none(),
            "a history-independent value must still be stored"
        );
    }

    /// Both the main search and quiescence must claim the fifty-move draw at the same boundary,
    /// and that boundary is 100 plies rather than 50. Quiescence once compared the clock against
    /// 50, reporting a draw at 25 moves — a whole half of the legal range in which the two searches
    /// disagreed about whether the game was already over.
    ///
    /// The sweep covers every clock across that former disagreement, from the old boundary to the
    /// real one, rather than sampling three points: the defect was a wrong constant, so the test
    /// that pins it has to walk the range the constant governs.
    #[test]
    fn both_searches_claim_the_fifty_move_draw_at_the_same_hundred_ply_boundary() {
        core::init::init_globals();

        // No captures and no checks, so quiescence stands pat unless the draw fires. The material
        // value is a queen and a pawn up, nowhere near zero, so a zero score can only be the claim.
        //
        // The pawn is what makes the main-search leg meaningful. Without it every white move is
        // quiet, so from clock 99 a one-ply search legitimately finds the draw on the next ply and
        // scores zero whether or not the root position is itself drawn. A pawn push resets the
        // clock, so below the boundary the search always has an escape and a zero score still means
        // only one thing.
        for halfmove_clock in 50..=100 {
            let fen = format!("4k3/8/8/8/8/8/P7/Q3K3 w - - {halfmove_clock} 1");
            let position = Position::from_fen(&fen).unwrap();
            let expected_draw = halfmove_clock >= 100;

            assert_eq!(
                position.fifty_move_rule_reached(),
                expected_draw,
                "the rule predicate disagreed at halfmove clock {halfmove_clock}"
            );

            let flag = AtomicBool::new(false);

            let quiescence_table = Table::new(1);
            let mut quiescence = Search::new(position.clone(), &flag, None, &quiescence_table);
            assert_eq!(
                quiescence.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P) == Some(Score::zero()),
                expected_draw,
                "quiescence disagreed at halfmove clock {halfmove_clock}"
            );

            let main_table = Table::new(1);
            let mut main = Search::new(position, &flag, None, &main_table);
            assert_eq!(
                main.run::<Master>(1).unwrap().score == Score::zero(),
                expected_draw,
                "the main search disagreed at halfmove clock {halfmove_clock}"
            );
        }
    }

    #[test]
    fn quiescence_does_not_use_a_search_score_as_static_evaluation() {
        core::init::init_globals();

        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        table.store(
            position.zobrist().0,
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

    /// Seeds an entry for a bare-king position whose true value is zero, so any non-zero score the
    /// search returns can only have come out of the table. Seeding directly rather than warming
    /// with a real search pins the cutoff path under test instead of depending on what a warming
    /// search happens to leave behind.
    fn score_from_seeded_entry(
        seeded: Score,
        bound: Bound,
        mov: &Move,
        depth: u8,
    ) -> (NodeResult, usize) {
        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        table.store(position.zobrist().0, seeded, depth, bound, mov);

        let mut search = Search::new(position, &flag, None, &table);
        search.search_depth = 4;
        search.pvt = PVTable::new(4);
        let score = search.search::<Master, NonPv>(Score::cp(299), Score::cp(300), 4);

        (score, search.trace.hash_collisions())
    }

    /// A checkmated or stalemated node, and every node whose moves all failed low, stores its value
    /// with no move at all. Gating the main search's score reuse on the presence of a playable
    /// stored move made exactly those entries — the most certain ones in the table — permanently
    /// unusable. Reuse must depend on the entry being verified, not on it carrying a move.
    #[test]
    fn a_verified_entry_without_a_move_still_cuts_off_the_main_search() {
        core::init::init_globals();

        let seeded = Score::cp(300);
        let (score, _) = score_from_seeded_entry(seeded, Bound::Exact, &Move::null(), 8);

        assert_eq!(
            score,
            Some(seeded),
            "a move-less entry deep enough to cut off was ignored"
        );
    }

    /// The same holds when the entry does carry a move but that move cannot be played here. The
    /// full-key check inside `Table::probe` is what establishes identity; an unplayable move only
    /// means the entry supplies no ordering hint, and is recorded as the genuine Zobrist collision
    /// it must be. Both searches therefore treat the score and the move independently.
    #[test]
    fn an_unplayable_stored_move_costs_ordering_but_not_the_score() {
        core::init::init_globals();

        // No piece stands on e4 in the seeded position, so this move is not playable there.
        let unplayable = Move::build(Square::E4, Square::E5, None, MoveType::QUIET);
        let seeded = Score::cp(300);
        let (score, collisions) = score_from_seeded_entry(seeded, Bound::Exact, &unplayable, 8);

        assert_eq!(
            score,
            Some(seeded),
            "an unplayable ordering move suppressed a verified score"
        );
        assert_eq!(
            collisions, 1,
            "an unplayable move on a verified entry must be counted as a collision"
        );
    }

    /// Quiescence must publish its results, not merely consume other nodes'. A quiet position has
    /// nothing to search, so the value it stores is its stand pat, recorded at the reserved
    /// quiescence draft.
    #[test]
    fn quiescence_publishes_its_result_at_the_reserved_draft() {
        core::init::init_globals();

        let position = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(position.clone(), &flag, None, &table);

        let score = search
            .quiesce::<Master, Pv>(Score::INF_N, Score::INF_P)
            .unwrap();

        let entry = table
            .probe(position.zobrist().0)
            .expect("quiescence must store a completed result");
        assert_eq!(entry.score(), score);
        assert_eq!(entry.depth(), Search::QUIESCENCE_DRAFT);
        assert_eq!(entry.bound(), Bound::Exact);
    }

    /// The reserved draft is what keeps the two searches' entries apart. A capture-only value can
    /// never satisfy a main-search node's depth requirement, so seeding one cannot change a
    /// depth-one search: the result must match a search that never saw the entry at all.
    #[test]
    fn a_quiescence_entry_cannot_satisfy_a_main_search_depth_requirement() {
        core::init::init_globals();

        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);

        let score_with = |table: &Table| {
            let mut search = Search::new(position.clone(), &flag, None, table);
            search.search_depth = 1;
            search.pvt = PVTable::new(1);
            search.search::<Master, NonPv>(Score::cp(299), Score::cp(300), 1)
        };

        let seeded = Table::new(1);
        seeded.store(
            position.zobrist().0,
            Score::cp(300),
            Search::QUIESCENCE_DRAFT,
            Bound::Exact,
            &Move::null(),
        );

        assert_eq!(
            score_with(&seeded),
            score_with(&Table::new(1)),
            "a quiescence-draft entry was reused by a depth-one main search"
        );
    }

    /// A quiescence subtree that a stop cut short has examined only some of its captures, so its
    /// alpha is not a bound on anything. It must not reach the table, on the same terms as the
    /// main search's aborted subtrees.
    #[test]
    fn an_aborted_quiescence_subtree_publishes_nothing() {
        core::init::init_globals();

        // A capture chain, so quiescence recurses rather than standing pat immediately, and the
        // abort lands part way through the tree.
        let position = Position::from_fen("4k3/8/8/3q4/4P3/5N2/8/4K3 w - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);
        search.root_fallback_ready = true;
        search.abort_after_nodes = Some(1);

        assert_eq!(
            search.quiesce::<Master, Pv>(Score::INF_N, Score::INF_P),
            None,
            "the abort must actually cut the subtree short"
        );
        assert!(
            table.probe(position.zobrist().0).is_none(),
            "an aborted quiescence subtree published an entry"
        );
    }

    /// Quiescence follows quiet check evasions, which advance the halfmove clock, so it can claim a
    /// fifty-move draw below its own root. That value depends on the moves played before the
    /// search, which the key does not cover, and is suppressed exactly as the main search
    /// suppresses its own.
    #[test]
    fn a_history_sensitive_quiescence_value_is_not_stored() {
        core::init::init_globals();

        // White is in check from the rook, so quiescence follows the evasions rather than standing
        // pat. Every evasion is a quiet king move, which advances the clock from 99 to the boundary
        // and makes the child claim the draw — below this node's own root, which is not yet drawn.
        let position = Position::from_fen("k3r3/8/8/8/8/8/8/4K3 w - - 99 1").unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(position.clone(), &flag, None, &table);

        search
            .quiesce::<Master, Pv>(Score::INF_N, Score::INF_P)
            .unwrap();

        assert!(
            search.history_draws > 0,
            "the test position must actually cross the fifty-move boundary below the root"
        );
        assert!(
            table.probe(position.zobrist().0).is_none(),
            "a clock-contaminated quiescence value was published"
        );
    }

    /// The point of the table is that a repeated search costs less and answers the same. Re-running
    /// each position against the table its own first search filled must not change the score or the
    /// move, and must not cost more nodes than the cold search did.
    #[test]
    fn a_warm_table_matches_the_cold_result_and_never_costs_more_nodes() {
        core::init::init_globals();

        let positions = [
            // A forced mate: terminal values, stored without a move, and the entries a move-gated
            // cutoff could never reuse.
            ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6),
            ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 6),
            // Tactical material wins, where the saving comes from ordinary bound reuse.
            ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 5),
            ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6),
        ];

        for (fen, depth) in positions {
            let position = Position::from_fen(fen).unwrap();
            let flag = AtomicBool::new(false);
            let table = Table::new(16);

            let mut cold = Search::new(position.clone(), &flag, None, &table);
            let cold_result = cold.run::<Master>(depth).unwrap();
            let cold_nodes = cold.trace.all_nodes_visited();

            let mut warm = Search::new(position, &flag, None, &table);
            let warm_result = warm.run::<Master>(depth).unwrap();
            let warm_nodes = warm.trace.all_nodes_visited();

            assert_eq!(
                warm_result.score, cold_result.score,
                "{fen}: a warm table changed the score"
            );
            assert_eq!(
                warm_result.best_move, cold_result.best_move,
                "{fen}: a warm table changed the best move"
            );
            assert!(
                warm_nodes <= cold_nodes,
                "{fen}: the warm search cost more nodes ({warm_nodes}) than the cold one \
                 ({cold_nodes})"
            );
        }
    }

    #[test]
    fn quiescence_ignores_tt_slot_clashes() {
        core::init::init_globals();

        let position = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let clashing_position = Position::from_fen("7k/8/8/8/8/8/8/K7 b - - 0 1").unwrap();
        let flag = AtomicBool::new(false);
        // The smallest table is a single cluster, so both positions necessarily share it.
        let table = Table::new(0);
        assert_eq!(
            table.cluster_index(position.zobrist().0),
            table.cluster_index(clashing_position.zobrist().0)
        );
        table.store(
            clashing_position.zobrist().0,
            Score::cp(300),
            8,
            Bound::Exact,
            &Move::null(),
        );
        assert!(
            table.probe(position.zobrist().0).is_none(),
            "another position's entry in the same cluster must not verify"
        );
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

        let mut engine = SearchEngine::new(1);
        let marker = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        assert_ne!(
            engine.table.cluster_index(marker.zobrist().0),
            engine
                .table
                .cluster_index(Position::start_pos().zobrist().0)
        );
        engine.table.store(
            marker.zobrist().0,
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
        assert!(engine.table.probe(marker.zobrist().0).is_some());

        // `clear_hash` needs an exclusive reference to the table, which is only obtainable once
        // every search has finished — the boundary that keeps a clear from racing a live worker.
        engine.clear_hash();
        assert!(engine.table.probe(marker.zobrist().0).is_none());
    }

    /// Dropping a handle rather than waiting on it must still leave the table unshared, so that a
    /// subsequent new-game clear can take its exclusive reference. If `Drop` merely cancelled and
    /// detached, the worker would outlive the handle still holding a clone of the table, and the
    /// clear below would panic whenever it won the race.
    #[test]
    fn dropping_a_search_handle_releases_the_table_for_a_later_clear() {
        core::init::init_globals();

        let mut engine = SearchEngine::new(1);

        // An unbounded search, so it is certainly still running at the point the handle is
        // dropped. Nothing observes its outcome: the drop is the whole subject of the test.
        drop(engine.start(Position::start_pos(), SearchLimit::Infinite));

        engine.clear_hash();
    }

    #[test]
    fn concurrent_searches_do_not_invalidate_the_shared_generation() {
        core::init::init_globals();

        let engine = SearchEngine::new(1);
        let marker = Position::from_fen("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        engine.table.store(
            marker.zobrist().0,
            Score::cp(17),
            1,
            Bound::Exact,
            &Move::null(),
        );

        let first = engine.start(Position::start_pos(), SearchLimit::Depth(2));
        let second = engine.start(Position::start_pos(), SearchLimit::Depth(2));
        first.wait();
        second.wait();

        assert!(engine.table.probe(marker.zobrist().0).is_some());
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

    /// Sweep the 300-position Win At Chess tactical suite and format every root score, at the
    /// depths where mate scores start appearing in quantity. This is the broad counterpart to the
    /// targeted window tests: it is not looking for a specific value but for any score the search
    /// can reach whose rendering panics, which is how a `Display` parity violation once surfaced.
    /// Debug assertions must be live for it to mean anything, so run it on a debug build:
    ///
    /// ```text
    /// cargo test -p engine -- --ignored wac_root_scores_format_without_panicking
    /// ```
    #[test]
    #[ignore = "sweeps 900 searches; run explicitly when changing Score or the search window"]
    fn wac_root_scores_format_without_panicking() {
        core::init::init_globals();

        let raw =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../suites/wac.epd"))
                .expect("wac.epd must be readable");

        // EPD records carry only the four placement fields, so the clocks are appended.
        let positions: Vec<(String, String)> = raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                let id = line
                    .split("id \"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap_or("unknown")
                    .to_string();
                let fen = format!(
                    "{} {} {} {} 0 1",
                    fields[0], fields[1], fields[2], fields[3]
                );
                (id, fen)
            })
            .collect();

        assert_eq!(positions.len(), 300, "the full WAC suite must be swept");

        let mut formatted = 0;
        for (id, fen) in &positions {
            for depth in [4, 5, 6] {
                let position = Position::from_fen(fen).unwrap();
                let engine = SearchEngine::new(1);
                let search = engine.start(position, SearchLimit::Depth(depth));
                let events = search.events().clone();
                let outcome = search.wait();

                assert!(
                    matches!(outcome, SearchOutcome::Completed(_)),
                    "{id} depth {depth} did not complete",
                );

                for event in events.try_iter() {
                    if let SearchEvent::Progress(progress) = &event {
                        assert!(
                            progress.score.is_node_score(),
                            "{id} depth {depth} reported {:?}, outside the node score band",
                            progress.score,
                        );
                        // `Display` carries the parity assertions; formatting is the check.
                        let line = crate::info::format_search_event(&event);
                        assert!(line.contains("score "), "{id} depth {depth}: {line}");
                        formatted += 1;
                    }
                }
            }
        }

        assert!(
            formatted >= positions.len() * 3,
            "expected at least one root score per search, got {formatted}",
        );
    }

    /// The window `(Score(20_100), Score(20_101))` is not contrived: it is exactly what a child
    /// receives when its parent searches the null window at the very bottom of the mate band,
    /// since `child_bound` is exact and both ends of that window sit above the top of the band.
    /// Every score is below such an alpha. The entry clamp keeps the threshold inside the node
    /// band. A collapsed window returns that in-band threshold before recursion; this is required
    /// bound sanitation rather than mate-distance pruning.
    #[test]
    fn out_of_band_windows_do_not_leak_into_returned_scores() {
        core::init::init_globals();

        let out_of_band_alpha = Score::from_i16(20_100);
        let out_of_band_beta = Score::from_i16(20_101);
        assert_eq!(Score::mate(0).child_bound(), out_of_band_beta);
        assert!(!out_of_band_alpha.is_node_score());
        assert!(!out_of_band_beta.is_node_score());

        for depth in [0, 1, 2] {
            let flag = AtomicBool::new(false);
            let table = Table::new(1);
            let (sender, _events) = unbounded();
            let mut search = Search::with_events(
                Position::from_fen("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61").unwrap(),
                &flag,
                None,
                &table,
                sender,
            );

            // Entering below the root, so `draft` is measured from this node rather than from a
            // root that was never searched.
            search.search_depth = depth;

            let value = search
                .search::<Master, NonPv>(out_of_band_alpha, out_of_band_beta, depth)
                .expect("an uncancelled search must produce a score");

            assert!(
                value.is_node_score(),
                "depth {depth} returned {value:?}, outside the node score band",
            );
            // The parent's view has to be well formed too, since that is what reaches `Display`.
            assert!(value.neg().inc_mate().is_node_score());
        }
    }

    /// The same window, entered directly at quiescence. Quiescence is where the excursion used to
    /// compound, because it had no window normalization to absorb an out-of-band bound and it
    /// returns `alpha` and `beta` themselves as fail-soft scores.
    #[test]
    fn quiescence_clamps_out_of_band_windows_into_the_node_score_band() {
        core::init::init_globals();

        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let (sender, _events) = unbounded();
        let mut search = Search::with_events(
            Position::from_fen("2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61").unwrap(),
            &flag,
            None,
            &table,
            sender,
        );

        let value = search
            .quiesce::<Master, NonPv>(Score::from_i16(20_100), Score::from_i16(20_101))
            .expect("an uncancelled quiescence search must produce a score");

        assert_eq!(value, Score::mate(1));
        assert!(value.is_node_score());
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
        let root_entry = table
            .probe(position.zobrist().0)
            .expect("the completed depth-one root must still be in the table");
        assert_eq!(root_entry.depth(), 1);
        assert_eq!(
            root_entry
                .mov()
                .expect("the root entry carries its best move")
                .to_move(&position),
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
            table.probe(position.zobrist().0).is_none(),
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
    fn the_time_deadline_is_suppressed_until_the_first_ply_completes() {
        core::init::init_globals();

        let position = Position::start_pos();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        // The deadline has already elapsed, and the root fallback is established, but only the
        // completed first ply may release the time-based abort.
        let mut search = Search::new(position, &flag, Some(Instant::now()), &table);
        search.root_fallback_ready = true;

        assert!(!search.stopping());

        search.min_search_complete = true;
        assert!(search.stopping());
    }

    #[test]
    fn cancellation_is_suppressed_only_until_the_root_fallback_exists() {
        core::init::init_globals();

        let position = Position::start_pos();
        let flag = AtomicBool::new(true);
        let table = Table::new(1);
        let mut search = Search::new(position, &flag, None, &table);

        // Nothing legal has been recorded yet, so cancellation cannot abort: doing so would forfeit
        // with `bestmove 0000`.
        assert!(!search.stopping());

        // The fallback alone releases the cancellation flag. Unlike the time deadline, it does not
        // wait for the first ply, so no unbounded quiescence tree stands between `stop` and the
        // abort.
        search.establish_root_fallback();
        assert!(!search.min_search_complete);
        assert!(search.stopping());
    }

    #[test]
    fn cancellation_is_not_throttled_with_the_deadline_clock() {
        core::init::init_globals();

        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(
            Position::start_pos(),
            &flag,
            Some(Instant::now() + Duration::from_secs(60)),
            &table,
        );
        search.establish_root_fallback();
        search.min_search_complete = true;

        // The deadline sample taken here throttles subsequent clock reads, but it must not defer
        // the cancellation flag: the very next check at the same node has to observe the stop.
        assert!(!search.stopping());
        flag.store(true, Ordering::Relaxed);
        assert!(
            search.stopping(),
            "cancellation must be read at the same node"
        );
    }

    #[test]
    fn expired_deadline_stays_latched_at_the_same_node() {
        core::init::init_globals();

        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(Position::start_pos(), &flag, Some(Instant::now()), &table);
        search.min_search_complete = true;

        assert!(search.stopping(), "the elapsed deadline must stop search");
        assert!(
            search.stopping(),
            "deadline expiry must remain latched during same-node unwind checks"
        );
    }

    #[test]
    fn time_limited_search_honors_the_budget_after_the_guaranteed_ply() {
        core::init::init_globals();

        let budget = Duration::from_millis(20);
        let started = Instant::now();
        let engine = SearchEngine::new(1);
        let search = engine.start(Position::start_pos(), SearchLimit::Time(budget));
        let outcome = search.wait();
        let elapsed = started.elapsed();

        // The search returns of its own accord (the deadline aborts it) rather than running to the
        // maximum depth, and it still reports a completed legal move.
        assert!(matches!(outcome, SearchOutcome::Completed(_)));
        let result = outcome.result().expect("a legal move must be returned");
        assert!(result.depth >= 1);
        // Release deadline checks are at most 8 nodes apart (one node in debug builds). The
        // additional 100 ms allows for a slow or descheduled CI worker while still catching a
        // missed or excessively coarse sample.
        assert!(
            elapsed <= budget + Duration::from_millis(100),
            "{budget:?} search exceeded deadline tolerance: {elapsed:?}"
        );
    }

    #[test]
    fn immediate_cancellation_returns_a_legal_move() {
        core::init::init_globals();

        let position = Position::start_pos();
        let engine = SearchEngine::new(1);
        let search = engine.start(position.clone(), SearchLimit::Infinite);
        search.cancel();
        let outcome = search.wait();

        // Cancellation may win the race before any root move is searched. The fallback means the
        // result is nonetheless a legal move rather than the `bestmove 0000` forfeit.
        assert!(outcome.was_cancelled());
        let best_move = outcome
            .result()
            .expect("a cancelled search must still report the root fallback")
            .best_move
            .expect("non-terminal position has a legal move");
        assert!(position.valid_move(&best_move));
    }

    /// A position whose depth-1 quiescence tree is large enough that searching it is plainly
    /// distinguishable from not searching it.
    const QUIESCENCE_HEAVY_FEN: &str =
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

    #[test]
    fn cancellation_stops_the_first_iteration_without_searching_it() {
        core::init::init_globals();

        let position = Position::from_fen(QUIESCENCE_HEAVY_FEN).unwrap();
        let table = Table::new(1);

        // The same first iteration, uncancelled, is the baseline this is measured against.
        let running = AtomicBool::new(false);
        let mut baseline = Search::new(position.clone(), &running, None, &table);
        let searched = baseline.run::<Master>(1).expect("first ply completes");
        let searched_nodes = baseline.trace.all_nodes_visited();
        assert!(searched_nodes > 1_000, "baseline must be a real search");

        // With cancellation already set, the search returns without visiting a single node: it
        // never enters the depth-1 quiescence tree, whose size has no practically small bound.
        // This is the deterministic form of "an explicit stop is honored promptly".
        let cancelled = AtomicBool::new(true);
        let mut search = Search::new(position.clone(), &cancelled, None, &table);
        let result = search
            .run::<Master>(1)
            .expect("cancellation must still yield the root fallback");

        assert_eq!(search.trace.all_nodes_visited(), 0);
        assert_eq!(result.depth, 0, "no iteration completed");
        let best_move = result.best_move.expect("root has legal moves");
        assert!(
            position.valid_move(&best_move),
            "fallback move must be legal"
        );
        assert!(searched.best_move.is_some());
    }

    #[test]
    fn the_root_fallback_tracks_the_best_searched_root_move() {
        core::init::init_globals();

        let position = Position::from_fen(QUIESCENCE_HEAVY_FEN).unwrap();
        let flag = AtomicBool::new(false);
        let table = Table::new(1);
        let mut search = Search::new(position, &flag, None, &table);

        let result = search.run::<Master>(2).expect("search completes");

        // Cancelling mid-ply reports this move, not the arbitrary first generated one: the fallback
        // is upgraded as each root move is fully searched.
        assert_eq!(search.root_fallback, result.best_move);
    }

    #[test]
    fn cancelled_terminal_root_reports_no_move() {
        core::init::init_globals();

        // Checkmate: there is no legal move to fall back to, so cancellation must not invent one.
        let position = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        let engine = SearchEngine::new(1);
        let search = engine.start(position, SearchLimit::Infinite);
        search.cancel();
        let outcome = search.wait();

        assert!(outcome
            .result()
            .is_none_or(|result| result.best_move.is_none()));
        assert_eq!(
            crate::info::format_search_outcome(&outcome),
            "bestmove 0000"
        );
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
