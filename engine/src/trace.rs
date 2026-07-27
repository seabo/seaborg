//! Utility for efficiently tracing data about the progress of a search, such as node visit counts
//! and nodes per second.

use std::ops::AddAssign;
use std::time::{Duration, Instant};

use super::killer::MAX_KILLER_SLOTS;

/// Object responsible for tracing data about the search.
pub struct Tracer {
    /// The time the search commenced.
    start_time: Instant,
    /// The number of nodes visited during search.
    nodes_visited: usize,
    /// The number of nodes visited during quiescence search.
    q_nodes_visited: usize,
    /// The number of nodes we skip due to a failed SEE check.
    see_skipped_nodes: usize,
    /// The number of probes that returned a verified entry.
    hash_hits: usize,
    /// The number of verified entries whose stored move was not legal in the probed position.
    ///
    /// The table verifies the full Zobrist key, so this counts genuine key collisions rather than
    /// the truncated-signature accidents a shorter signature would also admit. It overlaps
    /// `hash_hits`: a collision is a hit whose move could not be used.
    hash_collisions: usize,
    /// The number of probes that found no entry for the position.
    hash_misses: usize,
    /// Records the duration between start and end of search. Only populated with `Some(duration)`
    /// when `end_search` is called.
    elapsed: Option<Duration>,
    /// Killer moves searched, indexed by the recency slot they came from, counted after staged
    /// ordering has suppressed any killer that duplicated the hash move.
    ///
    /// This is effectiveness data, not availability: a slot is charged an attempt only when a killer
    /// from it was actually searched as a distinct move, and [`killer_cutoffs`](Self::killer_cutoffs)
    /// records how many of those attempts produced a beta cutoff. The ratio measures whether a slot
    /// earns its place in the ordering. Counting how often a killer was merely legal or offered would
    /// measure exposure instead, which is what this deliberately replaces.
    killer_attempts: [u64; MAX_KILLER_SLOTS],
    /// Beta cutoffs caused by a searched killer, indexed by the recency slot it came from, counted on
    /// the same post-suppression basis as [`killer_attempts`](Self::killer_attempts).
    killer_cutoffs: [u64; MAX_KILLER_SLOTS],
    pub hash_found: Averager<u32>,
    /// Off-by-default selectivity counters. Present only under the `selstats` feature so the shipped
    /// build carries neither the fields nor the increments that feed them.
    #[cfg(feature = "selstats")]
    sel: SelStats,
}

/// Per-search selectivity counters, accumulated only when the `selstats` feature is on.
///
/// These answer "where does the search spend its effective depth" from the engine's own point of
/// view: how early beta cutoffs land (move ordering), how often reductions and scouts are undone by
/// a re-search (reduction calibration), how wide quiescence runs, and how available the transposition
/// move is. They never influence a search decision — every counter is written after the decision it
/// observes — so a `selstats` build explores exactly the same tree as a default build.
#[cfg(feature = "selstats")]
#[derive(Default)]
pub struct SelStats {
    /// Main-search node entries at PV nodes (the root counts as a PV node) and at non-PV nodes.
    pub nodes_pv: u64,
    pub nodes_nonpv: u64,
    /// Beta cutoffs (fail-high nodes) and, of those, the ones that cut on the first move searched.
    /// `fh_first / fh_total` is the first-move-cutoff rate, the headline move-ordering signal.
    pub fh_total: u64,
    pub fh_first: u64,
    /// Cutoff move-index histogram: index `i` counts cutoffs on the `(i+1)`-th move searched, with
    /// the final bucket absorbing every later move. Shows how far down the ordering cutoffs migrate.
    pub fh_idx: [u64; 8],
    /// Fail-high (cutoff) counts split by node type, cross-checking `fh_total`.
    pub pv_fail_high: u64,
    pub nonpv_fail_high: u64,
    /// Non-cutoff outcomes of a completed move loop, split by node type: a PV node that raised alpha
    /// (an exact score) versus one that did not (fail-low), and non-PV fail-lows. Non-PV nodes are
    /// searched with a null window and so are never exact.
    pub pv_exact: u64,
    pub pv_fail_low: u64,
    pub nonpv_fail_low: u64,
    /// Late-move reductions applied (a reduced scout was searched), and of those how many the shallow
    /// verdict beat alpha and forced a full-depth re-search. `lmr_research / lmr_applied` is the
    /// re-search rate: too high means the reductions are too aggressive, near zero means too timid.
    pub lmr_applied: u64,
    pub lmr_research: u64,
    /// Sum of reduction plies over all applications (for the mean) and a histogram of the amount:
    /// index `i` counts a reduction of `i+1` plies, the last bucket absorbing larger reductions.
    pub lmr_red_sum: u64,
    pub lmr_red_hist: [u64; 6],
    /// PV-node scouts of a non-first move (searched first with a null window) and, of those, how many
    /// raised alpha and were re-searched with the full window. The PVS re-search rate.
    pub pv_scout: u64,
    pub pv_scout_research: u64,
    /// Root aspiration searches that ran with a finite window, and the fail-low / fail-high re-search
    /// events that widened it. `(asp_fail_low + asp_fail_high) / asp_windows` is the aspiration
    /// re-search rate.
    pub asp_windows: u64,
    pub asp_fail_low: u64,
    pub asp_fail_high: u64,
    /// Non-PV nodes short-circuited by a transposition-table entry before the move loop ran.
    pub tt_cutoffs: u64,
    /// Quiescence nodes that were in check and therefore widened from captures-only to every evasion.
    pub q_incheck: u64,
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracer {
    pub fn new() -> Self {
        Self {
            // Set to the time the struct was originally created for now. This will be updated
            // later with a call to `commence_search()`.
            start_time: Instant::now(),
            nodes_visited: 0,
            q_nodes_visited: 0,
            see_skipped_nodes: 0,
            hash_hits: 0,
            hash_collisions: 0,
            hash_misses: 0,
            elapsed: None,
            killer_attempts: [0; MAX_KILLER_SLOTS],
            killer_cutoffs: [0; MAX_KILLER_SLOTS],
            hash_found: Averager::new(0),
            #[cfg(feature = "selstats")]
            sel: SelStats::default(),
        }
    }

    /// Record that a killer from recency `slot` was searched as a distinct move.
    #[inline(always)]
    pub fn killer_attempt(&mut self, slot: usize) {
        self.killer_attempts[slot] += 1;
    }

    /// Record that a searched killer from recency `slot` produced a beta cutoff.
    #[inline(always)]
    pub fn killer_cutoff(&mut self, slot: usize) {
        self.killer_cutoffs[slot] += 1;
    }

    /// Killers searched per recency slot, after duplicate suppression.
    pub fn killer_attempts(&self) -> [u64; MAX_KILLER_SLOTS] {
        self.killer_attempts
    }

    /// Beta cutoffs caused by a searched killer per recency slot, after duplicate suppression.
    pub fn killer_cutoffs(&self) -> [u64; MAX_KILLER_SLOTS] {
        self.killer_cutoffs
    }

    /// To be called immediately before a new search commences. Used for timing of NPS measurements.
    pub fn commence_search(&mut self) {
        self.start_time = Instant::now();
    }

    /// To be called immediately after the search terminated. Used for timing of NPS measurements.
    pub fn end_search(&mut self) {
        self.elapsed = Some(self.start_time.elapsed())
    }

    /// To be called whenever the search visits a new node.
    #[inline(always)]
    pub fn visit_node(&mut self) {
        self.nodes_visited += 1;
    }

    /// To be called whenever the quiescence search visits a new node.
    #[inline(always)]
    pub fn visit_q_node(&mut self) {
        self.q_nodes_visited += 1;
    }

    /// To be called whenever we skip searching a node because it failed an SEE check.
    #[inline(always)]
    pub fn see_skip_node(&mut self) {
        self.see_skipped_nodes += 1;
    }

    /// Record a hash hit.
    #[inline(always)]
    pub fn hash_hit(&mut self) {
        self.hash_hits += 1;
    }

    /// Record a hash collision.
    #[inline(always)]
    pub fn hash_collision(&mut self) {
        self.hash_collisions += 1;
    }

    /// Record a probe that found nothing.
    #[inline(always)]
    pub fn hash_miss(&mut self) {
        self.hash_misses += 1;
    }

    /// The number of nodes skipped due to SEE check failures during search.
    pub fn see_skipped_nodes(&self) -> usize {
        self.see_skipped_nodes
    }

    /// The number of hash hits recorded during search.
    pub fn hash_hits(&self) -> usize {
        self.hash_hits
    }

    /// The number of hash collisions recorded during search.
    pub fn hash_collisions(&self) -> usize {
        self.hash_collisions
    }

    /// The number of hash misses recorded during search.
    pub fn hash_misses(&self) -> usize {
        self.hash_misses
    }

    /// The total number of hash probes, as the sum of hits and misses.
    ///
    /// Those two partition every probe, so their sum is the probe count. `hash_collisions` is
    /// deliberately not added: it counts a subset of the hits rather than a third outcome, so
    /// including it would count those probes twice.
    pub fn hash_probes(&self) -> usize {
        self.hash_hits + self.hash_misses
    }

    /// The number of nodes visited during main search.
    pub fn nodes_visited(&self) -> usize {
        self.nodes_visited
    }

    /// The number of nodes visited during quiescence search.
    pub fn q_nodes_visited(&self) -> usize {
        self.q_nodes_visited
    }

    /// The total number of nodes (main search _and_ quiescence) visited.
    pub fn all_nodes_visited(&self) -> usize {
        self.nodes_visited + self.q_nodes_visited
    }

    /// The nodes per second (NPS) of the search as at call-time. Calculated as total number of
    /// nodes visited so far divided by time since commence search was (last) called. This method
    /// should be used when reporting 'live' NPS from within an active search, as opposed to `nps`
    /// which is for reporting NPS after the end of a search.
    ///
    /// A search that completes in under a microsecond reports its rate as if it had taken one, so
    /// that a fast search reports a rate rather than dividing by zero.
    pub fn live_nps(&self) -> usize {
        nps_over(self.nodes_visited, self.start_time.elapsed())
    }

    /// Report the nodes per second (NPS) of the search process any time after it has terminated.
    /// This requires `end_search` to have been called at some point previously, and will return
    /// `None` if that is not the case.
    ///
    /// As with `live_nps`, a sub-microsecond search is treated as having taken one microsecond.
    pub fn nps(&self) -> Option<usize> {
        self.elapsed
            .map(|duration| nps_over(self.all_nodes_visited(), duration))
    }

    /// The time elapsed between start and end of the search.
    ///
    /// Returns `None` if `end_search` has never been called.
    pub fn elapsed(&self) -> Option<Duration> {
        self.elapsed
    }

    /// The time elapsed since the start of the search; for use when the search is still in
    /// progress.
    pub fn live_elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// The effective branching factor of this search. Note, this method uses a Newton-Raphson
    /// iteration. Although this often converges in a small number of iterations, it is probably
    /// best for performance to only call this at the end of a search, rather than during.
    ///
    /// If branching factor is x, and depth is d, and nodes searched is N, then the quantities
    /// satisfy (x^d - 1)/(x-1) = N. To solve this for x, we have to use a numerical method as
    /// there is no closed form rearrangement in terms of x.
    pub fn eff_branching(&self, depth: u8) -> f32 {
        eff_branching_factor(self.all_nodes_visited(), depth)
    }
}

/// Selectivity instrumentation, compiled only under the `selstats` feature. Every method observes a
/// decision the search has already made; none changes one, so a build with these calls active
/// searches the same tree as one without them.
#[cfg(feature = "selstats")]
impl Tracer {
    /// Record a main-search node entry, classified PV or non-PV.
    #[inline(always)]
    pub fn sel_node(&mut self, pv: bool) {
        if pv {
            self.sel.nodes_pv += 1;
        } else {
            self.sel.nodes_nonpv += 1;
        }
    }

    /// Record a beta cutoff at the given node type, cutting on the `move_count`-th move searched.
    #[inline(always)]
    pub fn sel_cutoff(&mut self, pv: bool, move_count: u32) {
        self.sel.fh_total += 1;
        if move_count == 1 {
            self.sel.fh_first += 1;
        }
        let bucket = (move_count.max(1) - 1).min(self.sel.fh_idx.len() as u32 - 1) as usize;
        self.sel.fh_idx[bucket] += 1;
        if pv {
            self.sel.pv_fail_high += 1;
        } else {
            self.sel.nonpv_fail_high += 1;
        }
    }

    /// Record the non-cutoff outcome of a completed move loop: for a PV node whether it raised alpha
    /// (an exact score) or not (a fail-low); a non-PV node's null-window loss is always a fail-low.
    #[inline(always)]
    pub fn sel_node_result(&mut self, pv: bool, raised_alpha: bool) {
        if pv {
            if raised_alpha {
                self.sel.pv_exact += 1;
            } else {
                self.sel.pv_fail_low += 1;
            }
        } else {
            self.sel.nonpv_fail_low += 1;
        }
    }

    /// Record a late-move reduction of `reduction` plies and whether its shallow verdict beat alpha
    /// and forced a full-depth re-search.
    #[inline(always)]
    pub fn sel_lmr(&mut self, reduction: u32, researched: bool) {
        self.sel.lmr_applied += 1;
        self.sel.lmr_red_sum += u64::from(reduction);
        let bucket = (reduction.max(1) - 1).min(self.sel.lmr_red_hist.len() as u32 - 1) as usize;
        self.sel.lmr_red_hist[bucket] += 1;
        if researched {
            self.sel.lmr_research += 1;
        }
    }

    /// Record a PV-node null-window scout of a non-first move.
    #[inline(always)]
    pub fn sel_pv_scout(&mut self) {
        self.sel.pv_scout += 1;
    }

    /// Record a full-window re-search after a PV-node scout raised alpha.
    #[inline(always)]
    pub fn sel_pv_research(&mut self) {
        self.sel.pv_scout_research += 1;
    }

    /// Record a root aspiration search that ran with a finite window.
    #[inline(always)]
    pub fn sel_asp_window(&mut self) {
        self.sel.asp_windows += 1;
    }

    /// Record an aspiration fail-low re-search.
    #[inline(always)]
    pub fn sel_asp_fail_low(&mut self) {
        self.sel.asp_fail_low += 1;
    }

    /// Record an aspiration fail-high re-search.
    #[inline(always)]
    pub fn sel_asp_fail_high(&mut self) {
        self.sel.asp_fail_high += 1;
    }

    /// Record a non-PV node short-circuited by a transposition-table entry before its move loop.
    #[inline(always)]
    pub fn sel_tt_cutoff(&mut self) {
        self.sel.tt_cutoffs += 1;
    }

    /// Record a quiescence node searched in check, which widens from captures to all evasions.
    #[inline(always)]
    pub fn sel_q_incheck(&mut self) {
        self.sel.q_incheck += 1;
    }

    /// Serialise the full selectivity profile for this search as one compact JSON object, tagged with
    /// the deepest completed `depth`. Combines the selectivity counters with the always-on node, TT,
    /// and killer figures so a single line captures the whole profile. Field order is stable so the
    /// analysis script can rely on it, though the values are keyed and read by name.
    pub fn sel_json(&self, depth: u8) -> String {
        use std::fmt::Write as _;

        // A search that visited no node (an immediate abort) leaves the branching factor and the
        // TT-move average as `0/0`. JSON has no NaN literal, so an unsanitised value would emit a
        // token the analysis parser rejects; both are reported as zero instead.
        let finite = |x: f64| if x.is_finite() { x } else { 0.0 };
        let ebf = f64::from(self.eff_branching(depth));

        let s = &self.sel;
        let mut out = String::with_capacity(768);
        out.push('{');
        let _ = write!(
            out,
            "\"depth\":{},\"nodes\":{},\"qnodes\":{},\"all_nodes\":{},\"ebf\":{:.4},",
            depth,
            self.nodes_visited(),
            self.q_nodes_visited(),
            self.all_nodes_visited(),
            finite(ebf),
        );
        let _ = write!(
            out,
            "\"hash_probes\":{},\"hash_hits\":{},\"hash_misses\":{},\"hash_collisions\":{},\"tt_move_avail\":{:.6},\"tt_cutoffs\":{},",
            self.hash_probes(),
            self.hash_hits(),
            self.hash_misses(),
            self.hash_collisions(),
            finite(self.hash_found.avg()),
            s.tt_cutoffs,
        );
        let _ = write!(
            out,
            "\"nodes_pv\":{},\"nodes_nonpv\":{},\"fh_total\":{},\"fh_first\":{},\"fh_idx\":{:?},",
            s.nodes_pv, s.nodes_nonpv, s.fh_total, s.fh_first, s.fh_idx,
        );
        let _ = write!(
            out,
            "\"pv_fail_high\":{},\"nonpv_fail_high\":{},\"pv_exact\":{},\"pv_fail_low\":{},\"nonpv_fail_low\":{},",
            s.pv_fail_high, s.nonpv_fail_high, s.pv_exact, s.pv_fail_low, s.nonpv_fail_low,
        );
        let _ = write!(
            out,
            "\"lmr_applied\":{},\"lmr_research\":{},\"lmr_red_sum\":{},\"lmr_red_hist\":{:?},",
            s.lmr_applied, s.lmr_research, s.lmr_red_sum, s.lmr_red_hist,
        );
        let _ = write!(
            out,
            "\"pv_scout\":{},\"pv_scout_research\":{},\"asp_windows\":{},\"asp_fail_low\":{},\"asp_fail_high\":{},\"q_incheck\":{}",
            s.pv_scout, s.pv_scout_research, s.asp_windows, s.asp_fail_low, s.asp_fail_high, s.q_incheck,
        );
        out.push('}');
        out
    }
}

/// Nodes per second for `nodes` visited over `elapsed`.
///
/// Durations are measured in whole microseconds, so a search faster than that measures as zero.
/// Such a search is charged one microsecond, both to avoid dividing by zero and because a rate
/// over a zero-length interval is not meaningful to report.
fn nps_over(nodes: usize, elapsed: Duration) -> usize {
    nodes * 1_000_000 / (elapsed.as_micros().max(1) as usize)
}

/// Used in Newton-Raphson iteration to calculate effective branching factor.
///
/// Represents the numerator in f(x_i)/f'(x_i)
fn numerator(b: f32, d: f32, n: f32) -> f32 {
    ((b.powf(d) - 1.) / (b - 1.)) - n
}

/// Used in Newton-Raphson iteration to calculate effective branching factor.
///
/// Represents the denominator in f(x_i)/f'(x_i)
fn denominator(b: f32, d: f32) -> f32 {
    ((d * b.powf(d - 1.) - 1.) * (b - 1.) - (b.powf(d) - 1.)) / (b - 1.).powf(2.)
}

/// Calculate the effective branching factor for a given number of nodes and a depth, using a
/// Newton-Raphson iteration.
pub fn eff_branching_factor(nodes: usize, depth: u8) -> f32 {
    let f_depth = Into::<f32>::into(depth);
    let n = nodes as f32;

    // Initial guess taken to be average branching factor for chess.
    let mut x: f32 = 38.;

    // We will use a delta between successive iterations to
    // determine when to stop.
    let mut last_delta;

    // The smallest enough delta between iterations for which we will return.
    let target_delta: f32 = 1e-3;

    // Sometimes, it can take a while to converge..
    let max_iterations = 100;

    for _ in 0..max_iterations {
        let x2 = x - numerator(x, f_depth, n) / denominator(x, f_depth);
        last_delta = (x2 - x).abs();

        if last_delta <= target_delta {
            return x;
        }

        x = x2;
    }

    x
}

/// Type for maintaining running averages of a quantity.
#[derive(Debug)]
pub struct Averager<T> {
    cum: T,
    cnt: usize,
}

impl<T> Averager<T>
where
    T: AddAssign + Into<f64> + Copy,
{
    /// Create a new `Averager` with initial value `init`.
    pub fn new(init: T) -> Self {
        Self { cum: init, cnt: 0 }
    }

    /// Push a `T` into the `Averager`.
    pub fn push(&mut self, val: T) {
        self.cum += val;
        self.cnt += 1;
    }

    /// Push multiple instances of `T` into the `Averager`. The function accepts the cumulative
    /// value of all the instances, and the number of instances.
    pub fn push_many(&mut self, val: T, cnt: usize) {
        self.cum += val;
        self.cnt += cnt;
    }

    /// Read the current average value from the `Averager`.
    pub fn avg(&self) -> f64 {
        Into::<f64>::into(self.cum) / (self.cnt as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A search fast enough to measure as zero microseconds must still report a rate. Both NPS
    /// accessors previously divided by that zero and panicked.
    #[test]
    fn nps_of_a_sub_microsecond_search_does_not_divide_by_zero() {
        assert_eq!(nps_over(3, Duration::ZERO), 3_000_000);
    }

    #[test]
    fn nps_is_a_per_second_rate() {
        assert_eq!(nps_over(0, Duration::ZERO), 0);
        assert_eq!(nps_over(7, Duration::from_micros(1)), 7_000_000);
        assert_eq!(nps_over(1_500, Duration::from_millis(1)), 1_500_000);
        assert_eq!(nps_over(4_200, Duration::from_secs(2)), 2_100);
    }
}
