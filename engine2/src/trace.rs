//! Utility for efficiently tracing data about the progress of a search, such as node visit counts
//! and nodes per second.

use std::time::Instant;

/// Object responsible for tracing data about the search.
pub struct Tracer {
    /// The time the search commenced.
    start_time: Instant,
    /// The number of nodes visited during search.
    nodes_visited: usize,
}

impl Tracer {
    pub fn new() -> Self {
        Self {
            // Set to the time the struct was originally created for now. This will be updated
            // later with a call to `commence_search()`.
            start_time: Instant::now(),
            nodes_visited: 0,
        }
    }

    /// To be called immediately before a new search commences. Used for timing of NPS measurements.
    pub fn commence_search(&mut self) {
        self.start_time = Instant::now();
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

    /// The nodes per second (NPS) of the search as of call-time. Calculated as total number of
    /// nodes visited so far divided by time since commence search was (last) called.
    pub fn nps(&self) -> usize {
        let elapsed = self.start_time.elapsed().as_micros();
        self.nodes_visited * 1_000_000 / (elapsed as usize)
    }
}
