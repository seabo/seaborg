//! Exponential backoff schedule shared by stream reconnects and rate-limit waits.

use std::time::Duration;

/// The first delay applied when a stream drops, before any doubling.
pub const RECONNECT_BASE: Duration = Duration::from_secs(1);
/// The ceiling a reconnect delay grows to, so a persistently unreachable server
/// is retried about twice a minute rather than ever more slowly.
pub const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// A doubling backoff bounded by a maximum delay.
///
/// The first delay is `base`; each subsequent delay doubles until it reaches
/// `max`, where it stays. A run of clean activity calls [`reset`](Backoff::reset)
/// to return to `base`, so a stream that reconnected cleanly a while ago does not
/// carry a long delay into its next, unrelated transient drop.
///
/// No jitter is applied: this is a single client reconnecting to one server, not
/// a fleet that could synchronize into a thundering herd.
#[derive(Debug, Clone)]
pub struct Backoff {
    base: Duration,
    max: Duration,
    next: Duration,
}

impl Backoff {
    /// Create a backoff that starts at `base` and never waits longer than `max`.
    pub fn new(base: Duration, max: Duration) -> Backoff {
        Backoff {
            base,
            max,
            next: base,
        }
    }

    /// Return to the initial delay after a successful stretch of activity.
    pub fn reset(&mut self) {
        self.next = self.base;
    }

    /// The delay to wait before the next attempt, advancing the schedule.
    ///
    /// Successive calls yield `base`, `2*base`, `4*base`, ... capped at `max`.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next.min(self.max);
        self.next = self.next.saturating_mul(2).min(self.max);
        delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_double_up_to_the_cap() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(8));
        let delays: Vec<u64> = (0..6).map(|_| backoff.next_delay().as_secs()).collect();
        // 1, 2, 4, then the 8s cap holds.
        assert_eq!(delays, vec![1, 2, 4, 8, 8, 8]);
    }

    #[test]
    fn reset_returns_to_the_base_delay() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));
        backoff.next_delay();
        backoff.next_delay();
        assert_eq!(backoff.next_delay(), Duration::from_secs(4));
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn a_base_above_the_cap_is_clamped() {
        // A misconfigured base larger than the cap still never exceeds the cap.
        let mut backoff = Backoff::new(Duration::from_secs(100), Duration::from_secs(10));
        assert_eq!(backoff.next_delay(), Duration::from_secs(10));
        assert_eq!(backoff.next_delay(), Duration::from_secs(10));
    }
}
