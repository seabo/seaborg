//! Shared, single-owner game state behind the loopback HTTP surface.
//!
//! [`GameController`] is a blocking single-owner type with no internal locking and no waker: it
//! only advances when [`GameController::poll`] is called. The session therefore owns it behind a
//! mutex, runs one driver thread that polls it, and republishes a serialized snapshot whenever the
//! state changes. Streaming clients never touch the controller; they wait on the published
//! snapshot alone, which keeps a slow or stalled browser from blocking the engine.

use super::wire::snapshot_to_json;
use crate::game::{CommandError, GameController};
use crate::search::SearchLimit;
use core::position::Player;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often the driver thread advances the controller.
///
/// The controller applies a finished engine move and surfaces search progress only from `poll`,
/// so this bounds how stale the published state can be while a search runs.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// The most recently published state, identified by a monotonic event ID.
///
/// The event ID is distinct from the game revision because search progress updates the snapshot
/// without advancing the revision; a stream keyed on the revision alone would miss them.
#[derive(Clone)]
struct Published {
    event_id: u64,
    json: Arc<str>,
}

/// Everything the HTTP layer shares across connections.
pub struct Session {
    controller: Mutex<GameController>,
    published: Mutex<Published>,
    updated: Condvar,
    token: String,
    running: AtomicBool,
}

impl Session {
    pub fn new(human_side: Player, search_limit: SearchLimit, hash_size_mb: usize) -> Arc<Self> {
        let controller = GameController::new(human_side, search_limit, hash_size_mb);
        let published = Published {
            event_id: 0,
            json: Arc::from(snapshot_to_json(&controller.snapshot()).as_str()),
        };
        Arc::new(Self {
            controller: Mutex::new(controller),
            published: Mutex::new(published),
            updated: Condvar::new(),
            token: generate_token(),
            running: AtomicBool::new(true),
        })
    }

    /// The per-process token that authorizes mutating requests.
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stop the driver thread and release every waiting stream.
    pub fn shutdown(&self) {
        // The flag is set under `published` even though it is atomic, because `wait_for_update`
        // reads it under that same lock before parking. Signalling without the lock would allow
        // the notify to land after that read but before `wait_timeout` parks — a condvar keeps no
        // record of a notification with no waiter, so it would be dropped and the stream would
        // sleep through shutdown until its keepalive timeout expired.
        //
        // That window is only a few instructions wide and the waiter holds this lock across all
        // of it, so it is not reachable from a test that drives the type from outside; attempting
        // one only produced a test that passed with the bug present. This is therefore justified
        // by the locking discipline rather than by a reproduction.
        {
            let _published = self.lock_published();
            self.running.store(false, Ordering::SeqCst);
        }
        self.updated.notify_all();
    }

    /// The currently published snapshot and its event ID.
    pub fn current(&self) -> (u64, Arc<str>) {
        let published = self.lock_published();
        (published.event_id, Arc::clone(&published.json))
    }

    /// Block until an event newer than `last_event_id` is published.
    ///
    /// Returns `None` on timeout or shutdown so the caller can send a keepalive or close.
    pub fn wait_for_update(
        &self,
        last_event_id: u64,
        timeout: Duration,
    ) -> Option<(u64, Arc<str>)> {
        let deadline = Instant::now() + timeout;
        let mut published = self.lock_published();
        loop {
            if published.event_id > last_event_id {
                return Some((published.event_id, Arc::clone(&published.json)));
            }
            if !self.is_running() {
                return None;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (guard, result) = self
                .updated
                .wait_timeout(published, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            published = guard;
            if result.timed_out() && published.event_id <= last_event_id {
                return None;
            }
        }
    }

    /// Advance the controller once, publishing if the snapshot changed.
    pub fn tick(&self) {
        let mut controller = self.lock_controller();
        if controller.poll() {
            self.publish(&controller);
        }
    }

    /// Apply a human move, publishing the result on success.
    pub fn play_move(&self, uci: &str, revision: u64) -> Result<(), CommandError> {
        let mut controller = self.lock_controller();
        let result = controller.play_human_move(uci, revision);
        if result.is_ok() {
            self.publish(&controller);
        }
        result
    }

    /// Undo the last full turn, publishing the result on success.
    pub fn undo(&self, revision: u64) -> Result<(), CommandError> {
        let mut controller = self.lock_controller();
        let result = controller.undo(revision);
        if result.is_ok() {
            self.publish(&controller);
        }
        result
    }

    /// Start a fresh game for `human_side`.
    pub fn new_game(&self, human_side: Player) {
        let mut controller = self.lock_controller();
        controller.reset(human_side);
        self.publish(&controller);
    }

    /// Serialize and publish the controller's current snapshot, waking every waiting stream.
    ///
    /// Callers hold the controller lock, so the lock order is always controller then published.
    fn publish(&self, controller: &GameController) {
        let json = snapshot_to_json(&controller.snapshot());
        let mut published = self.lock_published();
        published.event_id += 1;
        published.json = Arc::from(json.as_str());
        drop(published);
        self.updated.notify_all();
    }

    /// A panic inside a command handler must not wedge the whole server, and the controller
    /// remains internally consistent because every mutation is a single method call.
    fn lock_controller(&self) -> std::sync::MutexGuard<'_, GameController> {
        self.controller
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_published(&self) -> std::sync::MutexGuard<'_, Published> {
        self.published
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Run the controller driver until the session shuts down.
pub fn drive(session: Arc<Session>) {
    while session.is_running() {
        session.tick();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Generate the per-process session token.
///
/// `RandomState` is seeded once per thread from the operating system's randomness, which another
/// local process cannot observe. Hashing distinct per-call material through a fresh instance
/// yields unrelated outputs, so this needs no external dependency. The token is one layer: the
/// listener is loopback-only and `Host`/`Origin` are validated independently.
fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let marker = Box::new(0_u8);
    // The heap address contributes address-space layout randomization.
    let address = Box::as_ref(&marker) as *const u8 as usize;

    let mut token = String::with_capacity(32);
    for round in 0..2_u64 {
        let state = RandomState::new();
        let mut hasher = state.build_hasher();
        hasher.write_u128(nanos);
        hasher.write_usize(address);
        hasher.write_u32(std::process::id());
        hasher.write_u64(round);
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::json::parse;
    use core::init::init_globals;

    fn session() -> Arc<Session> {
        init_globals();
        Session::new(Player::WHITE, SearchLimit::Depth(1), 1)
    }

    /// Wait until the published snapshot satisfies `predicate`, or fail.
    fn wait_until(session: &Session, predicate: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            session.tick();
            let (_, json) = session.current();
            if predicate(&json) {
                return json.to_string();
            }
            assert!(Instant::now() < deadline, "timed out waiting; last: {json}");
            std::thread::yield_now();
        }
    }

    #[test]
    fn publishes_an_initial_snapshot_before_any_poll() {
        let session = session();
        let (event_id, json) = session.current();
        assert_eq!(event_id, 0);
        let value = parse(&json).unwrap();
        assert_eq!(value.get("revision").unwrap().as_u64(), Some(0));
        assert_eq!(value.get("humanSide").unwrap().as_str(), Some("white"));
    }

    #[test]
    fn a_successful_command_publishes_a_new_event() {
        let session = session();
        let (before, _) = session.current();
        session.play_move("e2e4", 0).unwrap();
        let (after, json) = session.current();
        assert!(after > before, "expected a new event id");
        let value = parse(&json).unwrap();
        assert_eq!(value.get("revision").unwrap().as_u64(), Some(1));
    }

    #[test]
    fn a_rejected_command_publishes_nothing() {
        let session = session();
        let (before, before_json) = session.current();
        assert_eq!(session.play_move("e2e5", 0), Err(CommandError::IllegalMove));
        assert_eq!(
            session.play_move("e2e4", 99),
            Err(CommandError::StaleRevision {
                expected: 0,
                received: 99
            })
        );
        assert_eq!(session.undo(0), Err(CommandError::NothingToUndo));
        let (after, after_json) = session.current();
        assert_eq!(after, before);
        assert_eq!(before_json, after_json);
    }

    #[test]
    fn the_driver_applies_the_engine_reply_and_publishes_it() {
        let session = session();
        session.play_move("e2e4", 0).unwrap();
        let json = wait_until(&session, |json| {
            parse(json)
                .unwrap()
                .get("revision")
                .unwrap()
                .as_u64()
                .is_some_and(|revision| revision >= 2)
        });
        let value = parse(&json).unwrap();
        assert_eq!(value.get("sideToMove").unwrap().as_str(), Some("white"));
        assert_eq!(
            value
                .get("engineStatus")
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str(),
            Some("idle")
        );
    }

    #[test]
    fn waiting_returns_promptly_when_an_update_arrives() {
        let session = session();
        let (start, _) = session.current();
        let waiter = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.wait_for_update(start, Duration::from_secs(10)))
        };
        // Publishing before the waiter parks is not a race: `wait_for_update` checks the
        // published event ID before waiting, so it returns either way.
        std::thread::sleep(Duration::from_millis(20));
        session.play_move("e2e4", 0).unwrap();

        let observed = waiter.join().unwrap().expect("waiter saw no update");
        assert!(observed.0 > start);
        assert_eq!(observed.1, session.current().1);
    }

    #[test]
    fn waiting_times_out_when_nothing_changes() {
        let session = session();
        let (current, _) = session.current();
        let waited = Instant::now();
        assert!(session
            .wait_for_update(current, Duration::from_millis(50))
            .is_none());
        assert!(waited.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn waiting_returns_immediately_for_a_stale_event_id() {
        let session = session();
        session.new_game(Player::WHITE);
        let (current, _) = session.current();
        let update = session
            .wait_for_update(current - 1, Duration::from_secs(5))
            .expect("expected the already-published event");
        assert_eq!(update.0, current);
    }

    #[test]
    fn shutdown_releases_waiting_streams() {
        let session = session();
        let (current, _) = session.current();
        let waiter = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.wait_for_update(current, Duration::from_secs(30)))
        };
        std::thread::sleep(Duration::from_millis(20));
        session.shutdown();
        assert!(!session.is_running());
        assert!(waiter.join().unwrap().is_none());
    }

    #[test]
    fn the_driver_thread_stops_after_shutdown() {
        let session = session();
        let driver = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || drive(session))
        };
        session.shutdown();
        driver.join().expect("driver thread did not stop");
    }

    #[test]
    fn tokens_are_long_hex_and_differ_between_sessions() {
        let first = generate_token();
        let second = generate_token();
        assert_eq!(first.len(), 32);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}
