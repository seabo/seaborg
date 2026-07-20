//! Cooperative shutdown signal.
//!
//! A [`Shutdown`] is the "please stop" flag the event loop and every game worker
//! poll at each stream boundary. On Ctrl-C the bot stops accepting new challenges
//! and lets its in-flight games wind down cleanly (resigning rather than dropping
//! a connection mid-move) instead of the process being killed outright.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A shared flag observed by the event loop and every game worker.
///
/// Two backings exist so one handle type serves both production and tests: an
/// owned flag ([`Shutdown::new`]) that tests trip directly, and a reference to a
/// process-global flag set by an OS signal handler ([`install_signal_handler`]),
/// which cannot own an `Arc` because a signal handler runs with no context of its
/// own. Both are cheap to clone and share across threads.
#[derive(Clone)]
pub struct Shutdown(Source);

#[derive(Clone)]
enum Source {
    Owned(Arc<AtomicBool>),
    Static(&'static AtomicBool),
}

impl Shutdown {
    /// Create an un-tripped, independently owned shutdown flag.
    pub fn new() -> Shutdown {
        Shutdown(Source::Owned(Arc::new(AtomicBool::new(false))))
    }

    /// Request shutdown. Idempotent; a second request is a no-op.
    pub fn request(&self) {
        self.flag().store(true, Ordering::SeqCst);
    }

    /// Whether shutdown has been requested.
    pub fn is_requested(&self) -> bool {
        self.flag().load(Ordering::SeqCst)
    }

    /// Sleep for up to `duration`, waking early if shutdown is requested.
    ///
    /// Backoff and rate-limit waits go through here so a long delay (a 429 can
    /// ask for a full minute) never leaves Ctrl-C unanswered for its whole span.
    pub fn sleep(&self, duration: Duration) {
        // Poll granularity: short enough that shutdown feels immediate, long
        // enough not to busy-wait through a minute-long rate-limit backoff.
        const STEP: Duration = Duration::from_millis(200);
        let mut remaining = duration;
        while !remaining.is_zero() && !self.is_requested() {
            let chunk = STEP.min(remaining);
            std::thread::sleep(chunk);
            remaining -= chunk;
        }
    }

    fn flag(&self) -> &AtomicBool {
        match &self.0 {
            Source::Owned(flag) => flag,
            Source::Static(flag) => flag,
        }
    }
}

impl Default for Shutdown {
    fn default() -> Shutdown {
        Shutdown::new()
    }
}

/// Install a SIGINT/SIGTERM handler that trips the returned [`Shutdown`].
///
/// The handler only stores into an `AtomicBool`, which is async-signal-safe, so a
/// second Ctrl-C is harmless. On non-Unix targets there is no portable signal
/// facility, so this returns a handle that is only ever tripped programmatically
/// (the bot then relies on the process being terminated by other means).
#[cfg(unix)]
pub fn install_signal_handler() -> Shutdown {
    static FLAG: AtomicBool = AtomicBool::new(false);

    extern "C" fn trip(_signal: libc::c_int) {
        FLAG.store(true, Ordering::SeqCst);
    }

    // libc's `signal` takes the handler as a numeric `sighandler_t`, so the
    // function is taken to a pointer and then to that integer, which is how the C
    // API is spelled.
    let handler = trip as *const () as libc::sighandler_t;

    // SAFETY: `trip` only performs an atomic store, which is async-signal-safe,
    // and installing a handler for these two signals has no other side effect.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }

    Shutdown(Source::Static(&FLAG))
}

/// No portable signal facility off Unix; the returned handle is tripped only by
/// explicit [`Shutdown::request`] calls.
#[cfg(not(unix))]
pub fn install_signal_handler() -> Shutdown {
    Shutdown::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_untripped_and_trips_on_request() {
        let shutdown = Shutdown::new();
        assert!(!shutdown.is_requested());
        shutdown.request();
        assert!(shutdown.is_requested());
    }

    #[test]
    fn clones_share_the_same_flag() {
        let shutdown = Shutdown::new();
        let clone = shutdown.clone();
        shutdown.request();
        assert!(clone.is_requested(), "a clone must observe the request");
    }

    #[test]
    fn sleep_returns_immediately_once_requested() {
        let shutdown = Shutdown::new();
        shutdown.request();
        // Already tripped: the loop body never sleeps, so this returns at once
        // even for an absurd duration.
        shutdown.sleep(Duration::from_secs(3600));
    }
}
