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

    pub fn commence_search(&mut self) {
        self.start_time = Instant::now();
    }

    #[inline(always)]
    pub fn visit_node(&mut self) {
        self.nodes_visited += 1;
    }
}
