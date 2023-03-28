//! Utility for efficiently tracing data about the progress of a search, such as node visit counts
//! and nodes per second.

use std::ops::AddAssign;
use std::time::{Duration, Instant};

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
    /// The number of times we had a hash hit which was useable to return immediately.
    hash_hits: usize,
    /// The number of times we had a hash collision.
    hash_collisions: usize,
    /// The number of times we had a hash clash (same table slot, different position).
    hash_clashes: usize,
    /// Records the duration between start and end of search. Only populated with `Some(duration)`
    /// when `end_search` is called.
    elapsed: Option<Duration>,
    pub killers_per_node: Averager<u32>,
    pub hash_found: Averager<u32>,
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
            hash_clashes: 0,
            elapsed: None,
            killers_per_node: Averager::new(0),
            hash_found: Averager::new(0),
        }
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

    /// Record a hash collisions.
    #[inline(always)]
    pub fn hash_collision(&mut self) {
        self.hash_collisions += 1;
    }

    /// Record a hash clash.
    #[inline(always)]
    pub fn hash_clash(&mut self) {
        self.hash_clashes += 1;
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

    /// The number of hash clashes recorded during search.
    pub fn hash_clashes(&self) -> usize {
        self.hash_clashes
    }

    /// The total number of hash probes, calculated as the sum of hits, collisions and clashes
    /// recorded.
    pub fn hash_probes(&self) -> usize {
        self.hash_hits + self.hash_collisions + self.hash_clashes
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
    pub fn live_nps(&self) -> usize {
        let elapsed = self.start_time.elapsed().as_micros();
        self.nodes_visited * 1_000_000 / (elapsed as usize)
    }

    /// Report the nodes per second (NPS) of the search process any time after it has terminated.
    /// This requires `end_search` to have been called at some point previously, and will return
    /// `None` if that is not the case.
    pub fn nps(&self) -> Option<usize> {
        self.elapsed.and_then(|duration| {
            Some(self.all_nodes_visited() * 1_000_000 / (duration.as_micros() as usize))
        })
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
