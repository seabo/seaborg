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

/// Number of remaining-depth buckets in the selectivity width profile. Interior search depth stays
/// well below this in the measured regimes; the final bucket absorbs anything deeper. Kept at or
/// below 32 so the array still derives `Default`.
#[cfg(feature = "selstats")]
const SEL_DEPTH_BUCKETS: usize = 24;

/// Number of candidate quiet-pruning rules the shadow counters evaluate. Each is scored without
/// acting on the search, so a run ranks the levers before any behaviour change is written.
#[cfg(feature = "selstats")]
pub const SHADOW_CANDIDATES: usize = 4;

/// For a quiet-phase move the live search is about to search, which candidate pruning rules *would*
/// have removed it. Non-acting: the result feeds the shadow counters only. `history` is the move's
/// combined quiet history — higher is better, and a negative value marks a move that has historically
/// failed to produce a cutoff. The rules deliberately span two families:
///
/// - C0 linear `keep = 3 + depth`  — a gentle move-count cap, extended to every remaining depth.
/// - C1 linear `keep = 2 + depth`  — a sharper linear cap.
/// - C2 `keep = 4 + depth^2/4`     — a flatter quadratic than the live `3 + depth^2/2`, all depths.
/// - C3 history: `hist < 0`, `depth <= 8`, past a three-move prefix — prunes below-average quiets in
///   the tree interior regardless of move count.
#[cfg(feature = "selstats")]
fn shadow_prune_mask(depth: i16, move_count: u32, history: i32) -> [bool; SHADOW_CANDIDATES] {
    let mc = i32::from(depth);
    let count = move_count as i32;
    [
        count > 3 + mc,
        count > 2 + mc,
        count > 4 + mc * mc / 4,
        depth <= 8 && move_count > 3 && history < 0,
    ]
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
    /// Nodes that reached the move loop (ran ordering), split by node type and by whether a
    /// transposition-table move was available to seed that ordering. These are the denominators for a
    /// per-node-type TT-move availability, measured only where ordering actually happens (a node
    /// short-circuited before its move loop is excluded, so a trivially-present TT move at an early
    /// cutoff does not inflate the figure).
    pub ord_pv_tt: u64,
    pub ord_pv_nott: u64,
    pub ord_nonpv_tt: u64,
    pub ord_nonpv_nott: u64,
    /// Beta cutoffs and first-move cutoffs split by whether the cutting node had a TT move. The gap
    /// between `fh_tt_first / fh_tt` and `fh_nott_first / fh_nott` is the ordering penalty a missing
    /// TT move actually costs — the quantity that says whether low TT-move availability matters.
    pub fh_tt: u64,
    pub fh_tt_first: u64,
    pub fh_nott: u64,
    pub fh_nott_first: u64,
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
    /// Per-remaining-depth tree-width profile over the LMP-eligible node population — non-PV nodes
    /// that are not in check, which is exactly where move-count and history pruning of the quiet tail
    /// could act. Indexed by remaining depth, the final bucket absorbing deeper nodes. `depth_nodes`
    /// counts such nodes that ran a move loop; `depth_moves` sums the moves actually recursed into
    /// (counted after the current move-count and futility prunes have already removed a tail), and
    /// `depth_quiets` the quiet subset of those. `depth_moves / depth_nodes` by depth shows where the
    /// tree stays wide: the residual quiet width at remaining depths past `LMP_MAX_DEPTH` is the
    /// untapped pruning opportunity these counters exist to size.
    pub depth_nodes: [u64; SEL_DEPTH_BUCKETS],
    pub depth_moves: [u64; SEL_DEPTH_BUCKETS],
    pub depth_quiets: [u64; SEL_DEPTH_BUCKETS],
    /// Shadow evaluation of candidate quiet-pruning rules (see [`shadow_prune_mask`]), scored on the
    /// quiet-phase moves the live search actually searched at LMP-eligible nodes — so every count is
    /// an *incremental* effect beyond today's pruning. `shadow_denom` is the number of such moves
    /// seen; `shadow_pruned[c]` how many candidate `c` would remove (coverage = tree savings).
    /// `shadow_good[c]` counts, among the quiet moves that actually raised alpha or forced the cutoff
    /// (`quiet_good_total` of them), how many candidate `c` would wrongly kill (damage = soundness
    /// cost). A good lever has high coverage and near-zero damage.
    pub shadow_denom: u64,
    pub shadow_pruned: [u64; SHADOW_CANDIDATES],
    pub quiet_good_total: u64,
    pub shadow_good: [u64; SHADOW_CANDIDATES],
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

    /// Record a node that reached its move loop (ran ordering), classified by node type and by
    /// whether a transposition-table move was available to seed that ordering.
    #[inline(always)]
    pub fn sel_node_ordering(&mut self, pv: bool, tt_move: bool) {
        match (pv, tt_move) {
            (true, true) => self.sel.ord_pv_tt += 1,
            (true, false) => self.sel.ord_pv_nott += 1,
            (false, true) => self.sel.ord_nonpv_tt += 1,
            (false, false) => self.sel.ord_nonpv_nott += 1,
        }
    }

    /// Record a beta cutoff at the given node type, cutting on the `move_count`-th move searched, at a
    /// node that did or did not have a TT move to order by.
    #[inline(always)]
    pub fn sel_cutoff(&mut self, pv: bool, move_count: u32, tt_move: bool) {
        self.sel.fh_total += 1;
        let first = move_count == 1;
        if first {
            self.sel.fh_first += 1;
        }
        let bucket = (move_count.max(1) - 1).min(self.sel.fh_idx.len() as u32 - 1) as usize;
        self.sel.fh_idx[bucket] += 1;
        if pv {
            self.sel.pv_fail_high += 1;
        } else {
            self.sel.nonpv_fail_high += 1;
        }
        if tt_move {
            self.sel.fh_tt += 1;
            self.sel.fh_tt_first += u64::from(first);
        } else {
            self.sel.fh_nott += 1;
            self.sel.fh_nott_first += u64::from(first);
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

    /// Record an LMP-eligible node (non-PV, not in check) that ran a move loop, keyed by its remaining
    /// depth, for the tree-width profile. This is the denominator for the per-depth width means.
    #[inline(always)]
    pub fn sel_depth_node(&mut self, depth: i16) {
        let bucket = depth.clamp(0, SEL_DEPTH_BUCKETS as i16 - 1) as usize;
        self.sel.depth_nodes[bucket] += 1;
    }

    /// Record a move that survived the current move-count and futility prunes and is about to be
    /// searched at an LMP-eligible node of the given remaining depth, noting whether it is quiet. The
    /// totals therefore measure the tree width that remains *after* today's pruning, so the residual
    /// quiet count is exactly what a deeper-reaching prune could still remove.
    #[inline(always)]
    pub fn sel_move_searched(&mut self, depth: i16, is_quiet: bool) {
        let bucket = depth.clamp(0, SEL_DEPTH_BUCKETS as i16 - 1) as usize;
        self.sel.depth_moves[bucket] += 1;
        if is_quiet {
            self.sel.depth_quiets[bucket] += 1;
        }
    }

    /// Score a quiet-phase move the live search searched at an LMP-eligible node against every
    /// candidate rule: bumps the coverage denominator and each candidate's would-prune count.
    #[inline(always)]
    pub fn sel_shadow_searched(&mut self, depth: i16, move_count: u32, history: i32) {
        self.sel.shadow_denom += 1;
        let mask = shadow_prune_mask(depth, move_count, history);
        for (c, &pruned) in mask.iter().enumerate() {
            if pruned {
                self.sel.shadow_pruned[c] += 1;
            }
        }
    }

    /// Score a quiet-phase move that raised alpha or forced the cutoff at an LMP-eligible node — a
    /// move no rule should prune, so each candidate that would is charged a soundness cost.
    #[inline(always)]
    pub fn sel_shadow_good(&mut self, depth: i16, move_count: u32, history: i32) {
        self.sel.quiet_good_total += 1;
        let mask = shadow_prune_mask(depth, move_count, history);
        for (c, &pruned) in mask.iter().enumerate() {
            if pruned {
                self.sel.shadow_good[c] += 1;
            }
        }
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
        let mut out = String::with_capacity(1024);
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
            "\"ord_pv_tt\":{},\"ord_pv_nott\":{},\"ord_nonpv_tt\":{},\"ord_nonpv_nott\":{},\"fh_tt\":{},\"fh_tt_first\":{},\"fh_nott\":{},\"fh_nott_first\":{},",
            s.ord_pv_tt, s.ord_pv_nott, s.ord_nonpv_tt, s.ord_nonpv_nott, s.fh_tt, s.fh_tt_first, s.fh_nott, s.fh_nott_first,
        );
        let _ = write!(
            out,
            "\"lmr_applied\":{},\"lmr_research\":{},\"lmr_red_sum\":{},\"lmr_red_hist\":{:?},",
            s.lmr_applied, s.lmr_research, s.lmr_red_sum, s.lmr_red_hist,
        );
        let _ = write!(
            out,
            "\"pv_scout\":{},\"pv_scout_research\":{},\"asp_windows\":{},\"asp_fail_low\":{},\"asp_fail_high\":{},\"q_incheck\":{},",
            s.pv_scout, s.pv_scout_research, s.asp_windows, s.asp_fail_low, s.asp_fail_high, s.q_incheck,
        );
        let _ = write!(
            out,
            "\"depth_nodes\":{:?},\"depth_moves\":{:?},\"depth_quiets\":{:?}",
            s.depth_nodes, s.depth_moves, s.depth_quiets,
        );
        let _ = write!(
            out,
            ",\"shadow_denom\":{},\"shadow_pruned\":{:?},\"quiet_good_total\":{},\"shadow_good\":{:?}",
            s.shadow_denom, s.shadow_pruned, s.quiet_good_total, s.shadow_good,
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
