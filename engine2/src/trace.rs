//! Utility for efficiently tracing data about the progress of a search, such as node visit counts
//! and nodes per second.

use std::time::{Duration, Instant};

/// Object responsible for tracing data about the search.
pub struct Tracer {
    /// The time the search commenced.
    start_time: Instant,
    /// The number of nodes visited during search.
    nodes_visited: usize,
    /// Records the duration between start and end of search. Only populated with `Some(duration)`
    /// when `end_search` is called.
    elapsed: Option<Duration>,
}

impl Tracer {
    pub fn new() -> Self {
        Self {
            // Set to the time the struct was originally created for now. This will be updated
            // later with a call to `commence_search()`.
            start_time: Instant::now(),
            nodes_visited: 0,
            elapsed: None,
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

    /// The number of nodes visited.
    pub fn nodes_visited(&self) -> usize {
        self.nodes_visited
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
            Some(self.nodes_visited * 1_000_000 / (duration.as_micros() as usize))
        })
    }

    /// The effective branching factor of this search. Note, this method uses a Newton-Raphson
    /// iteration. Although this usually converges in just 2-3 iterations, it is probably best for
    /// performance to only call this at the end of a search, rather than during.
    ///
    /// Calculated as (nodes * (depth - 1) + 1) ^ (1/depth).
    pub fn eff_branching(&self, depth: u8) -> f32 {
        let f_depth = Into::<f32>::into(depth + 1);
        let n = self.nodes_visited as f32;

        // Initial guess taken to be average branching factor for chess.
        let mut x: f32 = 35.;

        // We will use a delta between successive iterations to
        // determine when to stop.
        let mut last_delta;

        // The smallest enough delta between iterations for which we will return.
        let target_delta: f32 = 1e-1;

        // We should never need many. This converges very fast. Stop at 10 because if we have done
        // that many, something is going wrong.
        let max_iterations = 10;

        for _ in 0..max_iterations {
            let x2 = x - numerator(x, f_depth, n) / denominator(x, f_depth);
            last_delta = (x2 - x).abs();
            x = x2;

            if last_delta <= target_delta {
                return x;
            }
        }

        x
    }
}

/// Used in Newton-Raphson iteration to calculate effective branching factor.
///
/// Represents the numerator in f(x_i)/f'(x_i)
fn numerator(b: f32, d: f32, n: f32) -> f32 {
    (b.powf(d) - 1.) / (b - 1.) - n
}

/// Used in Newton-Raphson iteration to calculate effective branching factor.
///
/// Represents the denominator in f(x_i)/f'(x_i)
fn denominator(b: f32, d: f32) -> f32 {
    (d * b.powf(d - 1.) * (b - 1.) - b.powf(d) + 1.) / (b - 1.).powf(2.)
}
