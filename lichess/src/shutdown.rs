//! Cooperative shutdown signal.
//!
//! A [`Shutdown`] is the "please stop" flag the event loop and every game worker
//! poll at each stream boundary. It carries three stages rather than a single
//! bit, so an operator can stop the bot cleanly between games:
//!
//! - [`Stage::Running`]: normal operation.
//! - [`Stage::Draining`]: stop seeking and accepting new games, but let every
//!   in-flight game play to completion. Entered on the first Ctrl-C.
//! - [`Stage::ShuttingDown`]: wind down now — in-flight games resign (rather than
//!   dropping a connection mid-move) and threads join. Entered on a second Ctrl-C,
//!   or automatically once draining reaches zero active games.
//!
//! The stages only ever advance, so a later observation is never weaker than an
//! earlier one.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How far along shutdown has progressed. Ordered so that a numeric comparison
/// answers "at least this far": `Draining as u8` is below `ShuttingDown as u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Normal operation: seeking, accepting, and playing.
    Running = 0,
    /// Finishing in-flight games while starting no new ones.
    Draining = 1,
    /// Winding down immediately: in-flight games resign and threads join.
    ShuttingDown = 2,
}

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
    Owned(Arc<AtomicU8>),
    Static(&'static AtomicU8),
}

impl Shutdown {
    /// Create a shutdown handle in [`Stage::Running`].
    pub fn new() -> Shutdown {
        Shutdown(Source::Owned(Arc::new(AtomicU8::new(Stage::Running as u8))))
    }

    /// Enter [`Stage::Draining`], but only from [`Stage::Running`], so a drain
    /// request never pulls an already-immediate shutdown back to draining.
    /// Returns whether this call performed the transition, so the caller can
    /// announce the drain exactly once.
    pub fn begin_drain(&self) -> bool {
        self.flag()
            .compare_exchange(
                Stage::Running as u8,
                Stage::Draining as u8,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }

    /// Request immediate shutdown ([`Stage::ShuttingDown`]). Idempotent, and it
    /// escalates from any earlier stage, so a drain-to-zero or a second interrupt
    /// both land here.
    pub fn request(&self) {
        self.flag()
            .store(Stage::ShuttingDown as u8, Ordering::SeqCst);
    }

    /// The current stage.
    pub fn stage(&self) -> Stage {
        match self.flag().load(Ordering::SeqCst) {
            0 => Stage::Running,
            1 => Stage::Draining,
            _ => Stage::ShuttingDown,
        }
    }

    /// Whether immediate shutdown has been requested. This is the "resign and
    /// stop" signal the game workers, event reader, and transport poll: it is
    /// true only at [`Stage::ShuttingDown`], so a drain leaves them running.
    pub fn is_requested(&self) -> bool {
        self.stage() == Stage::ShuttingDown
    }

    /// Whether the bot is draining or further along — the signal to stop seeking
    /// and accepting new games while in-flight games continue.
    pub fn is_draining(&self) -> bool {
        self.stage() >= Stage::Draining
    }

    /// Sleep for up to `duration`, waking early once immediate shutdown is
    /// requested.
    ///
    /// Backoff and rate-limit waits go through here so a long delay (a 429 can
    /// ask for a full minute) never leaves an immediate shutdown unanswered for
    /// its whole span. A drain does not wake these waits: a draining bot keeps
    /// operating normally, so its backoffs run to completion.
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

    fn flag(&self) -> &AtomicU8 {
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

/// Install a SIGINT/SIGTERM handler that advances the returned [`Shutdown`].
///
/// The first interrupt enters [`Stage::Draining`]; any subsequent interrupt
/// escalates to [`Stage::ShuttingDown`]. The handler only reads and stores an
/// `AtomicU8`, which is async-signal-safe. On non-Unix targets there is no
/// portable signal facility, so this returns a handle that is only ever advanced
/// programmatically (the bot then relies on the process being terminated by other
/// means).
#[cfg(unix)]
pub fn install_signal_handler() -> Shutdown {
    static FLAG: AtomicU8 = AtomicU8::new(Stage::Running as u8);

    extern "C" fn trip(_signal: libc::c_int) {
        // Escalate one stage per interrupt: the first drains, a later one shuts
        // down now. A plain load-then-store is enough here: shutdown signals are
        // delivered serially in practice, and the only possible race — two
        // interrupts landing at once — escalates straight to ShuttingDown, which
        // is exactly what a second interrupt asks for.
        let next = if FLAG.load(Ordering::SeqCst) == Stage::Running as u8 {
            Stage::Draining as u8
        } else {
            Stage::ShuttingDown as u8
        };
        FLAG.store(next, Ordering::SeqCst);
    }

    // libc's `signal` takes the handler as a numeric `sighandler_t`, so the
    // function is taken to a pointer and then to that integer, which is how the C
    // API is spelled.
    let handler = trip as *const () as libc::sighandler_t;

    // SAFETY: `trip` only performs an atomic load and store, which are
    // async-signal-safe, and installing a handler for these two signals has no
    // other side effect.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }

    Shutdown(Source::Static(&FLAG))
}

/// No portable signal facility off Unix; the returned handle is advanced only by
/// explicit [`Shutdown::begin_drain`]/[`Shutdown::request`] calls.
#[cfg(not(unix))]
pub fn install_signal_handler() -> Shutdown {
    Shutdown::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_running_and_requests_shut_down() {
        let shutdown = Shutdown::new();
        assert_eq!(shutdown.stage(), Stage::Running);
        assert!(!shutdown.is_requested());
        assert!(!shutdown.is_draining());
        shutdown.request();
        assert_eq!(shutdown.stage(), Stage::ShuttingDown);
        assert!(shutdown.is_requested());
        // Immediate shutdown counts as draining too, so a drain-gated check still
        // fires once the bot is winding down.
        assert!(shutdown.is_draining());
    }

    #[test]
    fn drain_then_immediate_shutdown() {
        let shutdown = Shutdown::new();
        // First interrupt: drain. In-flight games keep going (not "requested"),
        // but new games are held back (draining).
        assert!(shutdown.begin_drain());
        assert_eq!(shutdown.stage(), Stage::Draining);
        assert!(shutdown.is_draining());
        assert!(!shutdown.is_requested());
        // Second interrupt: immediate shutdown.
        shutdown.request();
        assert_eq!(shutdown.stage(), Stage::ShuttingDown);
        assert!(shutdown.is_requested());
    }

    #[test]
    fn begin_drain_does_not_pull_back_an_immediate_shutdown() {
        let shutdown = Shutdown::new();
        shutdown.request();
        // A drain request arriving after an immediate shutdown must not weaken it.
        assert!(!shutdown.begin_drain());
        assert_eq!(shutdown.stage(), Stage::ShuttingDown);
        assert!(shutdown.is_requested());
    }

    #[test]
    fn begin_drain_is_idempotent() {
        let shutdown = Shutdown::new();
        assert!(
            shutdown.begin_drain(),
            "the first drain performs the transition"
        );
        assert!(
            !shutdown.begin_drain(),
            "a second drain reports no transition so it is announced once"
        );
        assert_eq!(shutdown.stage(), Stage::Draining);
    }

    #[test]
    fn clones_share_the_same_flag() {
        let shutdown = Shutdown::new();
        let clone = shutdown.clone();
        shutdown.begin_drain();
        assert!(clone.is_draining(), "a clone must observe the drain");
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
