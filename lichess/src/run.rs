//! Top-level entry points that wire configuration, transport, and the event
//! loop together for the `seaborg lichess` command.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::account::Account;
use crate::backoff::{Backoff, RECONNECT_BASE, RECONNECT_MAX};
use crate::client::LichessClient;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::event::{Challenge, Event};
use crate::game::{play_game, EngineMoveChooser};
use crate::matchmaking::{Action, Matchmaker};
use crate::policy::{self, Decision, DeclineReason};
use crate::shutdown::{self, Shutdown};
use crate::transport::{HttpTransport, Transport};

/// How many online bots to fetch when looking for a matchmaking opponent. A small
/// page is enough: the bot only needs one eligible opponent, and a fresh page is
/// fetched on each attempt.
const ONLINE_BOTS_LIMIT: u32 = 50;

/// How often the matchmaking thread wakes to consider seeking a game. The
/// [`Matchmaker`] gates the actual cadence (idle timeout, minimum interval), so
/// this only needs to be short enough that a due challenge is issued promptly
/// once the bot goes idle; it is not the challenge rate itself.
const MATCHMAKING_POLL: Duration = Duration::from_secs(1);

/// How long the event consumer waits for the next event before looping to
/// re-check the shutdown flag. Short enough that a shutdown request is honored
/// promptly on an otherwise-quiet stream, long enough not to busy-wait.
const CONSUMER_POLL: Duration = Duration::from_millis(200);

/// Read the bot token from the environment, failing fast when it is absent.
///
/// A whitespace-only value is treated as absent so a blank export does not sail
/// through to an authentication failure later.
pub fn load_token() -> Result<String> {
    match std::env::var(crate::TOKEN_ENV_VAR) {
        Ok(token) if !token.trim().is_empty() => Ok(token),
        _ => Err(Error::MissingToken),
    }
}

/// Connect to Lichess and run the challenge-handling event loop, playing each
/// accepted game in its own worker thread until Ctrl-C is pressed.
///
/// Ingestion is decoupled from outbound HTTP: the account event stream is read on
/// its own thread that only decodes lines onto a channel, a consumer thread
/// handles the decoded events (accept/decline and game lifecycle), and — when
/// enabled — matchmaking runs on a third thread. This keeps a rate-limit backoff
/// on an outgoing call (a challenge-create or bot-list `429` can pause a thread
/// for minutes) from stalling the reading of, and reaction to, an incoming
/// challenge; on the single-threaded design that backoff blocked the stream and
/// hung the challenger's UI.
///
/// The event stream and per-game streams drop routinely; both reconnect with
/// exponential backoff rather than ending the bot. Shutdown is two-stage: the
/// first Ctrl-C drains — the bot stops seeking and accepting games but lets every
/// in-flight game play to completion, exiting on its own once none remain — and a
/// second Ctrl-C winds down immediately, resigning in-flight games rather than
/// dropping their connections mid-move.
///
/// Fails fast when the token is missing or rejected, or when the authenticated
/// account is not a BOT account (which needs [`upgrade`] first).
pub fn run(config_path: Option<&Path>) -> Result<()> {
    let token = load_token()?;
    let config = Arc::new(Config::load(config_path)?);
    let shutdown = shutdown::install_signal_handler();
    let client = Arc::new(LichessClient::new(HttpTransport::new(
        crate::DEFAULT_BASE_URL,
        token,
        shutdown.clone(),
    )));

    let account = require_bot_account(&client)?;
    log::info!("connected to Lichess as bot {}", account.username);
    // Lichess reports each game's players by their lowercase account id, which
    // is what identifies the bot's own side once a game starts, and which the
    // account stream echoes as the challenger on the bot's own outgoing
    // challenges so the consumer can drop them.
    let bot_id = account.id;

    // Proactive matchmaking. Disabled by default, in which case the loop is
    // purely reactive; enabling it lets the bot challenge other bots when idle.
    // The matchmaker is shared with the event consumer (which records game
    // starts and declines) and, when enabled, the matchmaking thread, so it lives
    // behind a mutex; each holder locks it only for brief state updates, never
    // across an HTTP call.
    let matchmaker = Arc::new(Mutex::new(Matchmaker::new(
        config.matchmaking.clone(),
        config.max_concurrent_games,
        bot_id.clone(),
        Instant::now(),
    )));
    let matchmaking_enabled = matchmaker.lock().unwrap().is_enabled();
    if matchmaking_enabled {
        log::info!("matchmaking enabled: will challenge idle bots");
    }

    // Each accepted game runs to completion on its own thread, matching the
    // repo's std-thread idiom, so a slow search in one game cannot stall the
    // event loop or the other games. The handles are kept so shutdown can wait
    // for every worker to resign and exit rather than dropping mid-move.
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();

    // Every game slot the bot holds: reserved on accept, promoted when the game
    // starts, freed when it ends. This persists across event-stream reconnects, so
    // a `gameStart` replayed on reconnect does not spawn a second worker for a game
    // already in progress, and it is the source of truth for the concurrency cap.
    let slots = GameSlots::new();

    let spawn_game = |game_id: &str| -> std::thread::JoinHandle<()> {
        let client = Arc::clone(&client);
        let config = Arc::clone(&config);
        let shutdown = shutdown.clone();
        let slots = slots.clone();
        let bot_id = bot_id.clone();
        let game_id = game_id.to_string();
        std::thread::spawn(move || {
            let chooser = EngineMoveChooser::new(config.engine.hash_mb);
            if let Err(error) = play_game(&client, &config, &bot_id, &game_id, &chooser, &shutdown)
            {
                log::warn!("game {game_id} stopped on error: {error}");
            }
            // Free the cap slot when the game ends, however it ended. Doing this
            // from the worker keeps the count correct even if the matching
            // `gameFinish` event was missed while the event stream was down.
            slots.remove(&game_id);
        })
    };

    // A non-recoverable error raised by a background thread (the reader or the
    // matchmaker) is stored here and surfaced after everything winds down, so a
    // rejected token still ends the bot with an error rather than being lost to
    // the thread that hit it.
    let fatal: Arc<Mutex<Option<Error>>> = Arc::new(Mutex::new(None));

    // The event stream is read on its own thread that only decodes lines onto
    // this channel; because that thread makes no accept/decline/matchmaking
    // request, an outbound-call rate-limit backoff can never stall the reading of
    // an already-delivered event.
    let (events_tx, events_rx) = mpsc::channel::<Event>();

    let reader = {
        let client = Arc::clone(&client);
        let shutdown = shutdown.clone();
        let fatal = Arc::clone(&fatal);
        std::thread::spawn(move || {
            let result = run_event_reader(
                &client,
                &shutdown,
                |event| events_tx.send(event).is_ok(),
                |wait| shutdown.sleep(wait),
            );
            if let Err(error) = result {
                log::error!("event stream reader stopped: {error}");
                *fatal.lock().unwrap() = Some(error);
                shutdown.request();
            }
        })
    };

    // Matchmaking runs on its own thread so a rate-limit backoff on an outgoing
    // challenge cannot delay handling of an incoming one. It is spawned only when
    // matchmaking is enabled; the reactive-only path runs no such thread.
    let matchmaker_thread = if matchmaking_enabled {
        let client = Arc::clone(&client);
        let slots = slots.clone();
        let matchmaker = Arc::clone(&matchmaker);
        let shutdown = shutdown.clone();
        let fatal = Arc::clone(&fatal);
        Some(std::thread::spawn(move || {
            let result = run_matchmaking(&client, &slots, &matchmaker, &shutdown, |wait| {
                shutdown.sleep(wait)
            });
            if let Err(error) = result {
                log::error!("matchmaking stopped: {error}");
                *fatal.lock().unwrap() = Some(error);
                shutdown.request();
            }
        }))
    } else {
        None
    };

    // This thread consumes decoded events and handles them (accept/decline and
    // game-lifecycle tracking), spawning a worker per accepted game.
    let consumed = run_event_consumer(
        &client,
        &config,
        &bot_id,
        &slots,
        &matchmaker,
        &shutdown,
        events_rx,
        |game_id| workers.push(spawn_game(game_id)),
    );

    // However the loop ended, wind everything down: request shutdown so any
    // in-flight game resigns and the background threads leave their waits, then
    // join every thread before returning so the process does not exit while a
    // connection is still open.
    shutdown.request();
    let _ = reader.join();
    if let Some(handle) = matchmaker_thread {
        let _ = handle.join();
    }
    for worker in workers {
        let _ = worker.join();
    }

    // Prefer a consumer-side error; otherwise surface any fatal a background
    // thread recorded. A clean shutdown leaves both empty.
    consumed.and_then(|()| match fatal.lock().unwrap().take() {
        Some(error) => Err(error),
        None => Ok(()),
    })
}

/// Confirm the authenticated account is a BOT account, returning it.
///
/// Generic over the transport so the non-bot rejection can be tested with a
/// recorded account response instead of a live connection.
fn require_bot_account<T: Transport>(client: &LichessClient<T>) -> Result<Account> {
    let account = client.account()?;
    if account.is_bot() {
        Ok(account)
    } else {
        Err(Error::NotBotAccount {
            username: account.username,
        })
    }
}

/// The result of an upgrade attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeOutcome {
    /// The account was upgraded to a BOT account.
    Upgraded,
    /// The account was already a BOT account; nothing to do.
    AlreadyBot,
    /// The user declined the confirmation prompt.
    Cancelled,
}

/// Upgrade the authenticated account to a BOT account.
///
/// `confirm` is called with the account only once the upgrade is known to be
/// possible (the account exists, is not already a bot, and has zero games); it
/// returns whether the user approved the irreversible change. Keeping the prompt
/// in the caller lets this function stay free of terminal I/O and be tested with
/// a canned decision.
pub fn upgrade<F>(confirm: F) -> Result<UpgradeOutcome>
where
    F: FnOnce(&Account) -> bool,
{
    let token = load_token()?;
    // A one-shot command with no long-lived streams: an untripped shutdown handle
    // is all the transport needs.
    let client = LichessClient::new(HttpTransport::new(
        crate::DEFAULT_BASE_URL,
        token,
        Shutdown::new(),
    ));
    upgrade_account(&client, confirm)
}

/// The upgrade decision logic, generic over the transport so it can be tested
/// with a recorded account response and a canned confirmation.
fn upgrade_account<T, F>(client: &LichessClient<T>, confirm: F) -> Result<UpgradeOutcome>
where
    T: Transport,
    F: FnOnce(&Account) -> bool,
{
    let account = client.account()?;
    if account.is_bot() {
        return Ok(UpgradeOutcome::AlreadyBot);
    }
    let games = account.games_played();
    if games > 0 {
        return Err(Error::UpgradeIneligible {
            username: account.username,
            games,
        });
    }

    if !confirm(&account) {
        return Ok(UpgradeOutcome::Cancelled);
    }

    client.upgrade_to_bot()?;
    Ok(UpgradeOutcome::Upgraded)
}

/// Whether a game slot is merely reserved or backed by a running game.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotState {
    /// A challenge was accepted; the slot is held while awaiting its `gameStart`.
    /// No worker exists yet. Counts against the concurrency cap so a burst of
    /// accepts cannot overshoot it before any game has started.
    Reserved,
    /// The game has started and has a live worker.
    Active,
}

/// The concurrency-cap bookkeeping: every game slot the bot is committed to,
/// whether reserved by a just-accepted challenge or backing a running game.
///
/// Keyed by id. A slot's key is the challenge id at reservation and the game id
/// once it starts; Lichess makes these the same value, which is what lets a
/// `gameStart` reconcile the reservation its accept created instead of
/// double-counting it.
///
/// Shared between the event consumer — which reserves a slot on accept, promotes
/// it on `gameStart`, and reads the count for the cap — the matchmaking thread —
/// which reads the count to know whether a slot is free — and each game worker,
/// which removes its own game when it exits. Worker-driven removal keeps the cap
/// correct even if a `gameFinish` event is missed while the event stream is
/// disconnected, which the event-driven count alone could not guarantee.
#[derive(Clone, Default)]
pub struct GameSlots(Arc<Mutex<HashMap<String, SlotState>>>);

impl GameSlots {
    /// An empty set of slots.
    pub fn new() -> GameSlots {
        GameSlots::default()
    }

    /// Reserve a slot for a challenge about to be accepted, returning whether it
    /// was newly reserved. A `false` means the id is already held (reserved or
    /// active), so the caller must not reserve or accept it a second time.
    fn reserve(&self, id: &str) -> bool {
        let mut slots = self.0.lock().unwrap();
        if slots.contains_key(id) {
            return false;
        }
        slots.insert(id.to_string(), SlotState::Reserved);
        true
    }

    /// Mark a game as started, returning whether the caller should spawn a worker
    /// for it. A slot reserved at accept time is promoted in place — reconciling
    /// the reservation rather than adding a second slot — and a game the bot never
    /// reserved (an accepted outgoing matchmaking challenge) is recorded fresh;
    /// both spawn a worker. A game already active spawns nothing, so a `gameStart`
    /// replayed on reconnect does not start a duplicate worker.
    fn start(&self, id: &str) -> bool {
        let mut slots = self.0.lock().unwrap();
        match slots.get(id) {
            Some(SlotState::Active) => false,
            _ => {
                slots.insert(id.to_string(), SlotState::Active);
                true
            }
        }
    }

    /// Release a slot only if it is still merely reserved, for a challenge that was
    /// canceled or whose accept failed. A slot already promoted to a running game
    /// is left untouched, so a stray event cannot free an in-progress game's slot.
    fn release_reservation(&self, id: &str) {
        let mut slots = self.0.lock().unwrap();
        if slots.get(id) == Some(&SlotState::Reserved) {
            slots.remove(id);
        }
    }

    /// Drop `id` from the set unconditionally, for a finished game. Idempotent, so
    /// the worker and a `gameFinish` event removing the same game is harmless.
    fn remove(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }

    /// How many slots are held in total, reserved and active alike — the number
    /// the concurrency cap is measured against.
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// Whether no slot is held — no game reserved or running. This going true
    /// while draining is what lets the bot exit cleanly with no forfeits.
    fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
}

/// Read the account event stream and forward each decoded event, reconnecting
/// with exponential backoff when the stream drops.
///
/// This performs no accept/decline or matchmaking HTTP: it only decodes lines and
/// hands each real event to `forward`, so an outbound-call rate-limit backoff
/// running elsewhere cannot delay ingestion. `forward` returns `false` when the
/// consumer has gone away, which ends the reader. Blank keepalive lines are
/// consumed (giving the loop a chance to notice shutdown on a quiet stream) but
/// not forwarded. `sleep` performs the reconnect wait, injected so tests can
/// avoid real delays. Returns cleanly once shutdown is requested or the consumer
/// stops; a non-recoverable error surfaces.
///
/// Generic over the transport so it can be driven by recorded NDJSON in tests.
fn run_event_reader<T, F, P>(
    client: &LichessClient<T>,
    shutdown: &Shutdown,
    mut forward: F,
    mut sleep: P,
) -> Result<()>
where
    T: Transport,
    F: FnMut(Event) -> bool,
    P: FnMut(Duration),
{
    let mut backoff = Backoff::new(RECONNECT_BASE, RECONNECT_MAX);
    loop {
        if shutdown.is_requested() {
            return Ok(());
        }
        match read_stream_once(client, shutdown, &mut forward)? {
            StreamOutcome::Stop => return Ok(()),
            StreamOutcome::Disconnected { made_progress } => {
                if shutdown.is_requested() {
                    return Ok(());
                }
                // A connection that delivered events before dropping counts as
                // healthy, so its next unrelated drop backs off from the base.
                if made_progress {
                    backoff.reset();
                }
                log::warn!("event stream disconnected; reconnecting");
                sleep(backoff.next_delay());
            }
        }
    }
}

/// Why a single event-stream connection stopped.
enum StreamOutcome {
    /// Shutdown was requested, or the consumer stopped receiving; the reader
    /// should end.
    Stop,
    /// The connection ended without a fatal error. `made_progress` is whether any
    /// event arrived before it dropped.
    Disconnected { made_progress: bool },
}

/// Consume one event-stream connection until it drops, a fatal error occurs, the
/// consumer goes away, or shutdown is requested, forwarding each decoded event.
fn read_stream_once<T, F>(
    client: &LichessClient<T>,
    shutdown: &Shutdown,
    forward: &mut F,
) -> Result<StreamOutcome>
where
    T: Transport,
    F: FnMut(Event) -> bool,
{
    let stream = match client.event_stream() {
        Ok(stream) => stream,
        Err(error) if error.is_recoverable() => {
            return Ok(StreamOutcome::Disconnected {
                made_progress: false,
            })
        }
        Err(error) => return Err(error),
    };

    let mut made_progress = false;
    for item in stream {
        if shutdown.is_requested() {
            return Ok(StreamOutcome::Stop);
        }
        match item {
            Ok(Some(event)) => {
                made_progress = true;
                if !forward(event) {
                    // The consumer has gone; there is no reason to keep reading.
                    return Ok(StreamOutcome::Stop);
                }
            }
            // Keepalive line: nothing to forward, but the shutdown check at the
            // top of the loop still runs each line so a quiet but live stream
            // notices a shutdown request promptly.
            Ok(None) => {}
            Err(error) if error.is_recoverable() => {
                return Ok(StreamOutcome::Disconnected { made_progress })
            }
            Err(error) => return Err(error),
        }
    }
    Ok(StreamOutcome::Disconnected { made_progress })
}

/// Receive decoded events and act on each until the reader closes the channel or
/// shutdown is requested.
///
/// `bot_id` is the authenticated account's own id, used to ignore the bot's own
/// outgoing challenges echoed back on the stream. `start_game` is invoked with a
/// game's id the first time that game starts, to begin playing it. `slots` tracks
/// reserved and running games; it gates the concurrency cap and survives
/// reconnects so a replayed `gameStart` never spawns a duplicate worker. The loop
/// returns cleanly once shutdown is requested or the reader thread ends (closing
/// the channel); a non-recoverable error surfaces.
///
/// Acceptance is deferred rather than decided the instant a challenge arrives: a
/// challenge the policy permits is buffered, and once the burst of currently
/// available events has drained, the buffer is processed in priority order so a
/// human can be preferred over a bot and human-reserved slots held open. All the
/// events waiting on the channel at once form one such burst, matching the short
/// window Lichess challenges arrive in.
///
/// Generic over the transport so it can be driven with a test double.
// The consumer drives the full set of collaborators an event may touch (client,
// config, own identity, slot set, matchmaker, shutdown) plus the event channel
// and an injected `start_game` closure that exists so tests can substitute game
// spawning. The closure cannot join a plain data struct, so the argument count is
// inherent here rather than a sign of a missing abstraction.
#[allow(clippy::too_many_arguments)]
fn run_event_consumer<T, S>(
    client: &LichessClient<T>,
    config: &Config,
    bot_id: &str,
    slots: &GameSlots,
    matchmaker: &Mutex<Matchmaker>,
    shutdown: &Shutdown,
    events: Receiver<Event>,
    mut start_game: S,
) -> Result<()>
where
    T: Transport,
    S: FnMut(&str),
{
    // Challenges the policy permits but that have not yet been accepted, held only
    // for the span of one drain-and-process pass so they can be sorted against one
    // another before any slot is claimed.
    let mut pending: Vec<Challenge> = Vec::new();
    // The drain-entry message is logged once, the first time this loop observes
    // that the bot has begun draining.
    let mut drain_announced = false;
    while !shutdown.is_requested() {
        if shutdown.is_draining() {
            if !drain_announced {
                log::info!("{}", drain_message(slots.len()));
                drain_announced = true;
            }
            // Draining reaches zero when the last in-flight game's worker frees
            // its slot. With no game left to protect, escalate to an immediate
            // shutdown so the loop exits and every thread joins — a clean exit
            // with no forfeits.
            if slots.is_empty() {
                shutdown.request();
                break;
            }
        }
        match events.recv_timeout(CONSUMER_POLL) {
            Ok(event) => {
                handle_event(
                    client,
                    config,
                    bot_id,
                    slots,
                    matchmaker,
                    shutdown,
                    &mut start_game,
                    &mut pending,
                    event,
                )?;
                // Absorb the rest of the burst so simultaneously-arriving
                // challenges are weighed together, then decide the whole batch.
                while let Ok(event) = events.try_recv() {
                    handle_event(
                        client,
                        config,
                        bot_id,
                        slots,
                        matchmaker,
                        shutdown,
                        &mut start_game,
                        &mut pending,
                        event,
                    )?;
                }
                process_accept_queue(client, config, slots, &mut pending)?;
            }
            // No event this interval: loop to re-check the shutdown flag and the
            // drain-to-zero condition.
            Err(RecvTimeoutError::Timeout) => {}
            // The reader thread has ended and closed the channel; nothing more
            // will arrive, so stop.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// Periodically seek a matchmaking game until shutdown or drain.
///
/// Runs on its own thread so a rate-limit backoff incurred while listing bots or
/// creating a challenge cannot delay the event consumer. `sleep` performs the
/// inter-poll wait, injected so tests can drive it without real delays. A
/// non-recoverable error surfaces; a transient one is logged inside the seek and
/// swallowed.
///
/// Seeking stops as soon as the bot begins draining: draining means "start no new
/// games", so there is nothing left for this thread to do and it exits, letting
/// the in-flight games it already started play out under their own workers.
fn run_matchmaking<T, P>(
    client: &LichessClient<T>,
    slots: &GameSlots,
    matchmaker: &Mutex<Matchmaker>,
    shutdown: &Shutdown,
    mut sleep: P,
) -> Result<()>
where
    T: Transport,
    P: FnMut(Duration),
{
    loop {
        if shutdown.is_draining() {
            return Ok(());
        }
        seek_matchmaking_game(client, slots, matchmaker, shutdown)?;
        sleep(MATCHMAKING_POLL);
    }
}

/// If matchmaking is due, fetch online bots, pick an eligible opponent, and issue
/// a challenge.
///
/// Does nothing unless matchmaking is enabled and the [`Matchmaker`] judges the
/// bot idle enough to seek a game. The matchmaker mutex is held only to decide
/// and to record the outcome, never across the bot-list or challenge-create HTTP,
/// so a rate-limit backoff on either call does not block the event consumer's
/// brief matchmaker updates. A transient failure to list bots or issue the
/// challenge is logged and swallowed so one bad request does not end the bot; a
/// non-recoverable error still surfaces.
fn seek_matchmaking_game<T: Transport>(
    client: &LichessClient<T>,
    slots: &GameSlots,
    matchmaker: &Mutex<Matchmaker>,
    shutdown: &Shutdown,
) -> Result<()> {
    // Once draining, issue nothing further even if the poll thread was already
    // mid-loop when the drain began, so no new challenge goes out during shutdown.
    if shutdown.is_draining() {
        return Ok(());
    }
    let now = Instant::now();
    // Read the slots count before locking the matchmaker so the two mutexes are
    // never held at once.
    let active_games = slots.len() as u32;
    {
        let mut matchmaker = matchmaker.lock().unwrap();
        if !matchmaker.is_enabled() {
            return Ok(());
        }
        if matchmaker.choose(now, active_games) != Action::Seek {
            return Ok(());
        }
        // Count this as an attempt up front so a failed lookup or an empty
        // candidate list still waits out the minimum interval before retrying.
        matchmaker.record_attempt(now);
    }

    let bots = match client.online_bots(ONLINE_BOTS_LIMIT) {
        Ok(bots) => bots,
        Err(error) if error.is_recoverable() => {
            log::warn!("listing online bots for matchmaking: {error}");
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    let (spec, target) = {
        let mut matchmaker = matchmaker.lock().unwrap();
        let spec = matchmaker.compose_spec();
        let target = matchmaker
            .select_opponent(&spec, &bots, now)
            .map(|bot| bot.id.clone());
        (spec, target)
    };
    let Some(target) = target else {
        return Ok(());
    };

    log::info!(
        "challenging bot {target} to {}+{} ({})",
        spec.initial_seconds,
        spec.increment_seconds,
        if spec.rated { "rated" } else { "casual" }
    );
    match client.create_challenge(&target, &spec) {
        Ok(()) => matchmaker.lock().unwrap().record_issued(now),
        Err(error) if error.is_recoverable() => {
            log::warn!("challenging bot {target}: {error}");
            // The challenge did not take (commonly a creation-time rejection).
            // Back off from this bot so the deterministic first-eligible selection
            // does not re-pick it every interval and wedge matchmaking on one
            // unreachable opponent.
            matchmaker
                .lock()
                .unwrap()
                .record_challenge_failed(&target, now);
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Act on one account event: buffer or decline a challenge by policy, or track a
/// game's lifecycle. A challenge the policy permits is pushed onto `pending` for
/// [`process_accept_queue`] to weigh against the rest of the burst; one the policy
/// rejects is declined at once, since it never competes for a slot. A transient
/// failure to decline is logged and swallowed so one bad request does not end the
/// bot; a non-recoverable error (a rejected token) still surfaces.
// The handler touches every collaborator an event may reach (client, config, own
// identity, slot set, matchmaker, shutdown, the spawn closure, and the accept
// buffer) plus the event itself; the closure prevents folding these into one
// struct, so the argument count is inherent rather than a missing abstraction.
#[allow(clippy::too_many_arguments)]
fn handle_event<T, S>(
    client: &LichessClient<T>,
    config: &Config,
    bot_id: &str,
    slots: &GameSlots,
    matchmaker: &Mutex<Matchmaker>,
    shutdown: &Shutdown,
    start_game: &mut S,
    pending: &mut Vec<Challenge>,
    event: Event,
) -> Result<()>
where
    T: Transport,
    S: FnMut(&str),
{
    match event {
        Event::Challenge { challenge } => {
            // The account stream echoes the bot's own outgoing challenges. Trying
            // to accept one is a request Lichess rejects with a 404, so drop these
            // before any policy decision — the bot neither accepts nor declines a
            // challenge it issued itself.
            if challenge.is_from_self(bot_id) {
                log::debug!("ignoring own outgoing challenge {}", challenge.id);
                return Ok(());
            }
            // While draining, start no new game: decline every incoming challenge
            // so the challenger is answered rather than left waiting on a bot that
            // is shutting down. Already-running games are untouched — they are
            // tracked by their workers, not this buffer.
            if shutdown.is_draining() {
                log::info!(
                    "declining challenge {} from {} (draining)",
                    challenge.id,
                    challenge.challenger.name
                );
                tolerate_recoverable(
                    client.decline_challenge(&challenge.id, DeclineReason::Generic),
                    || format!("declining challenge {}", challenge.id),
                )?;
                return Ok(());
            }
            match policy::classify(&challenge, &config.challenge) {
                // Suitable: defer to the accept queue so it can be ordered against
                // any other challenges in the same burst and checked against the
                // cap and human reservations when a slot is actually claimed.
                Decision::Accept => pending.push(challenge),
                // Unsuitable regardless of load: decline now, in arrival order.
                Decision::Decline(reason) => {
                    log::info!(
                        "declining challenge {} from {} ({})",
                        challenge.id,
                        challenge.challenger.name,
                        reason.as_str()
                    );
                    tolerate_recoverable(client.decline_challenge(&challenge.id, reason), || {
                        format!("declining challenge {}", challenge.id)
                    })?;
                }
            }
        }
        Event::GameStart { game } => {
            // A game is starting, so any matchmaking challenge that was pending is
            // resolved (this may be that challenge being accepted).
            matchmaker
                .lock()
                .unwrap()
                .record_game_started(Instant::now());
            if slots.start(&game.id) {
                log::info!(
                    "game {} started ({}/{} slots)",
                    game.id,
                    slots.len(),
                    config.max_concurrent_games
                );
                start_game(&game.id);
            } else {
                // A `gameStart` for a game already being played. The event stream
                // replays in-progress games when it reconnects, so ignore the
                // duplicate rather than spawning a second worker for it.
                log::debug!("ignoring duplicate gameStart for game {}", game.id);
            }
        }
        Event::GameFinish { game } => {
            slots.remove(&game.id);
            log::info!("game {} finished ({} slots)", game.id, slots.len());
        }
        Event::ChallengeDeclined { challenge } => {
            // A bot we challenged declined. Record it so matchmaking backs off
            // from re-challenging that bot for the configured window.
            if let Some(dest) = challenge.dest_user {
                log::info!("bot {} declined our challenge", dest.id);
                matchmaker
                    .lock()
                    .unwrap()
                    .record_declined(&dest.id, Instant::now());
            }
        }
        Event::ChallengeCanceled { challenge } => {
            // The challenger withdrew before the game began. Free any slot the
            // accept path reserved for it so the reservation does not hold a slot
            // shut until it would have expired. If it was never reserved (declined,
            // or already promoted to a game) this is a no-op.
            slots.release_reservation(&challenge.id);
            log::debug!("challenge {} canceled by challenger", challenge.id);
        }
        Event::Other => {}
    }
    Ok(())
}

/// Accept the buffered challenges that fit, in priority order, declining the rest
/// for capacity.
///
/// Called once the current burst of events has drained, so every challenge that
/// arrived together is weighed as a group. When the policy prefers humans they are
/// sorted ahead of bots (a stable sort keeps arrival order within each group);
/// otherwise arrival order stands. Each challenge is then taken in turn: a bot may
/// occupy a slot only below `max_concurrent_games - reserved_human_slots`, holding
/// the remaining slots open for humans, while a human may use the full cap. A
/// challenge that fits is reserved and accepted; one that does not is declined for
/// capacity. Reserving before the accept POST means a concurrent matchmaking check
/// already sees the slot as taken, and the reservation is released if the accept
/// fails or the challenge has since vanished (a benign 404). The buffer is emptied
/// either way.
fn process_accept_queue<T: Transport>(
    client: &LichessClient<T>,
    config: &Config,
    slots: &GameSlots,
    pending: &mut Vec<Challenge>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    if config.challenge.prefer_human_challenges {
        // `false` (human) sorts before `true` (bot); the sort is stable, so
        // arrival order is preserved within humans and within bots.
        pending.sort_by_key(|challenge| challenge.challenger.is_bot());
    }
    let max_bot_games = config
        .max_concurrent_games
        .saturating_sub(config.matchmaking.reserved_human_slots);
    for challenge in pending.drain(..) {
        let effective_cap = if challenge.challenger.is_bot() {
            max_bot_games
        } else {
            config.max_concurrent_games
        };
        if (slots.len() as u32) < effective_cap {
            log::info!(
                "accepting challenge {} from {}",
                challenge.id,
                challenge.challenger.name
            );
            // Reserve before the POST so the slot counts against the cap for the
            // rest of this batch and for a concurrent matchmaking check.
            slots.reserve(&challenge.id);
            match client.accept_challenge(&challenge.id) {
                Ok(()) => {}
                // The challenge was canceled or expired before the accept landed —
                // the spec's challenge-gone outcome. Free the slot and move on;
                // this is expected, not a fault, so it is not logged as a warning.
                Err(Error::NotFound) => {
                    log::debug!("challenge {} gone before accept (404)", challenge.id);
                    slots.release_reservation(&challenge.id);
                }
                // A transient failure: free the slot and let the challenge lapse,
                // as with any recoverable error.
                Err(error) if error.is_recoverable() => {
                    log::warn!("accepting challenge {}: {error}", challenge.id);
                    slots.release_reservation(&challenge.id);
                }
                // A terminal fault (a rejected token) surfaces; free the slot first
                // so the count is honest as the bot winds down.
                Err(error) => {
                    slots.release_reservation(&challenge.id);
                    return Err(error);
                }
            }
        } else {
            // No slot for this challenger kind: a bot held out of the reserved
            // human slots, or any challenger over the full cap. Decline for
            // capacity so the challenger is not left waiting on a silent bot.
            log::info!(
                "declining challenge {} from {} (at capacity)",
                challenge.id,
                challenge.challenger.name
            );
            tolerate_recoverable(
                client.decline_challenge(&challenge.id, DeclineReason::Generic),
                || format!("declining challenge {}", challenge.id),
            )?;
        }
    }
    Ok(())
}

/// The operator-facing message logged when the bot enters drain mode, stating how
/// many in-flight games will still be played to completion and that another
/// interrupt quits immediately. Kept as a pure function so the wording — the
/// count and the second-interrupt hint an operator relies on — is asserted
/// directly, without capturing log output.
fn drain_message(remaining: usize) -> String {
    format!(
        "draining: finishing {remaining} in-flight game(s) and starting no new ones; interrupt again to quit immediately"
    )
}

/// Swallow a recoverable error from a challenge action, logging it with a context
/// message built lazily by `context`; propagate anything non-recoverable.
fn tolerate_recoverable(result: Result<()>, context: impl FnOnce() -> String) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.is_recoverable() => {
            log::warn!("{}: {error}", context());
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashSet, VecDeque};
    use std::sync::mpsc::Sender;
    use std::sync::Condvar;

    use super::*;
    use crate::config::Config;
    use crate::transport::Transport;

    /// A recorded POST: the request path and its form fields (empty for bodiless
    /// posts).
    type RecordedPost = (String, Vec<(String, String)>);

    /// A [`Transport`] that replays a canned account response and one recorded
    /// event stream per connection (in order, so a reconnect is fed the next
    /// stream), recording the POSTs the bot makes. It never touches the network,
    /// so challenge handling can be asserted deterministically.
    struct FakeTransport {
        account_json: String,
        /// NDJSON returned for `GET /api/bot/online`, for the matchmaking tests.
        bots_json: String,
        /// When set, an outgoing challenge-create POST fails with a recoverable
        /// HTTP error, standing in for a Lichess creation-time rejection so the
        /// failure-recovery path can be exercised offline.
        challenge_create_fails: bool,
        /// Challenge ids whose accept POST answers with HTTP 404, standing in for a
        /// challenge that was canceled or expired before the accept landed.
        accept_not_found: HashSet<String>,
        streams: RefCell<VecDeque<String>>,
        posts: RefCell<Vec<RecordedPost>>,
    }

    impl FakeTransport {
        fn new(account_json: &str, stream: &str) -> FakeTransport {
            FakeTransport::with_streams(account_json, [stream])
        }

        fn with_streams<'a>(
            account_json: &str,
            streams: impl IntoIterator<Item = &'a str>,
        ) -> FakeTransport {
            FakeTransport {
                account_json: account_json.to_string(),
                bots_json: String::new(),
                challenge_create_fails: false,
                accept_not_found: HashSet::new(),
                streams: RefCell::new(streams.into_iter().map(str::to_string).collect()),
                posts: RefCell::new(Vec::new()),
            }
        }

        /// The request paths POSTed, in order.
        fn post_paths(&self) -> Vec<String> {
            self.posts.borrow().iter().map(|(p, _)| p.clone()).collect()
        }

        /// How many recorded streams remain unopened.
        fn streams_remaining(&self) -> usize {
            self.streams.borrow().len()
        }
    }

    impl Transport for FakeTransport {
        fn get(&self, path: &str) -> Result<String> {
            if path.starts_with("/api/bot/online") {
                return Ok(self.bots_json.clone());
            }
            assert_eq!(path, "/api/account", "unexpected GET in test");
            Ok(self.account_json.clone())
        }

        fn post_empty(&self, path: &str) -> Result<String> {
            self.posts.borrow_mut().push((path.to_string(), Vec::new()));
            // A configured accept answers with 404, the challenge-gone outcome.
            if let Some(id) = path
                .strip_prefix("/api/challenge/")
                .and_then(|rest| rest.strip_suffix("/accept"))
            {
                if self.accept_not_found.contains(id) {
                    return Err(Error::NotFound);
                }
            }
            Ok(String::new())
        }

        fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
            let form = form
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            self.posts.borrow_mut().push((path.to_string(), form));
            // A challenge-create POST addresses a bot directly (`/api/challenge/{id}`),
            // unlike the accept/decline sub-actions on an existing challenge.
            let is_challenge_create = path.starts_with("/api/challenge/")
                && !path.ends_with("/accept")
                && !path.ends_with("/decline");
            if self.challenge_create_fails && is_challenge_create {
                return Err(Error::Http(
                    "unexpected status 400: {\"error\":\"nope\"}".to_string(),
                ));
            }
            Ok(String::new())
        }

        fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
            assert_eq!(path, "/api/stream/event", "unexpected stream path in test");
            let stream = self
                .streams
                .borrow_mut()
                .pop_front()
                .expect("event loop opened more connections than it recorded streams");
            // `str::lines` yields empty strings for the blank keepalive lines,
            // exercising the loop's tolerance of them.
            let lines: Vec<Result<String>> =
                stream.lines().map(|line| Ok(line.to_string())).collect();
            Ok(Box::new(lines.into_iter()))
        }
    }

    // A standard, casual 5+3 challenge from a human — accepted by the defaults.
    const ACCEPTABLE_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"good01","rated":false,"variant":{"key":"standard"},"timeControl":{"type":"clock","limit":300,"increment":3},"challenger":{"id":"alice","name":"alice","rating":1500,"title":null}}}"#;

    // A Chess960 challenge — declined by the default standard-only policy.
    const VARIANT_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"bad960","rated":false,"variant":{"key":"chess960"},"timeControl":{"type":"clock","limit":300,"increment":3},"challenger":{"id":"bob","name":"bob","rating":1600,"title":null}}}"#;

    // The authenticated bot's own account id, used to recognise its own outgoing
    // challenges echoed back on the account stream. None of the incoming-challenge
    // fixtures use it as the challenger, so it never falsely flags a real one.
    const SELF_ID: &str = "seaborg";

    /// Drive every event of a single recorded connection through [`handle_event`]
    /// for a bot whose own id is `bot_id`, returning the game ids the runner was
    /// asked to start. The accept queue is processed after each event, modeling
    /// events that arrive far enough apart to be handled one at a time — so a
    /// challenge is accepted the moment it is seen, before any later event.
    /// Matchmaking is disabled, isolating the accept/decline and game-lifecycle
    /// handling from outgoing challenges.
    fn handle_one_stream(client: &LichessClient<FakeTransport>, bot_id: &str) -> Vec<String> {
        let slots = GameSlots::new();
        let matchmaker = Mutex::new(Matchmaker::disabled());
        let shutdown = Shutdown::new();
        let config = Config::default();
        let mut pending = Vec::new();
        let mut started = Vec::new();
        let stream = client.event_stream().unwrap();
        for item in stream {
            if let Some(event) = item.unwrap() {
                handle_event(
                    client,
                    &config,
                    bot_id,
                    &slots,
                    &matchmaker,
                    &shutdown,
                    &mut |id: &str| started.push(id.to_string()),
                    &mut pending,
                    event,
                )
                .unwrap();
                process_accept_queue(client, &config, &slots, &mut pending).unwrap();
            }
        }
        started
    }

    /// Run the matchmaking seek `times` times against `client`, as the
    /// matchmaking thread would across successive polls.
    fn seek_times(
        client: &LichessClient<FakeTransport>,
        slots: &GameSlots,
        matchmaker: &Mutex<Matchmaker>,
        times: usize,
    ) {
        // A running (non-draining) handle: these scenarios exercise seeking under
        // normal operation.
        let shutdown = Shutdown::new();
        for _ in 0..times {
            seek_matchmaking_game(client, slots, matchmaker, &shutdown).unwrap();
        }
    }

    /// One outbound challenge-API call the bot made, classified from a recorded
    /// POST so scenarios can assert intent — with ids and decline reasons — rather
    /// than matching raw request paths.
    #[derive(Debug, PartialEq, Eq)]
    enum OutboundCall {
        /// `POST /api/challenge/{id}/accept`.
        Accept { id: String },
        /// `POST /api/challenge/{id}/decline` with a reason.
        Decline { id: String, reason: String },
        /// `POST /api/challenge/{username}` — an outgoing challenge-create.
        Create { username: String },
        /// `POST /api/challenge/{id}/cancel` — cancelling an outgoing challenge.
        Cancel { id: String },
    }

    /// Classify a recorded challenge POST into a typed [`OutboundCall`].
    ///
    /// Every path the event loop POSTs is under `/api/challenge/`; the trailing
    /// segment (or its absence) distinguishes accept, decline, cancel, and a bare
    /// create addressed to a username.
    fn classify_post((path, form): &RecordedPost) -> OutboundCall {
        let rest = path
            .strip_prefix("/api/challenge/")
            .unwrap_or_else(|| panic!("unexpected outbound POST during replay: {path}"));
        if let Some(id) = rest.strip_suffix("/accept") {
            OutboundCall::Accept { id: id.to_string() }
        } else if let Some(id) = rest.strip_suffix("/decline") {
            let reason = form
                .iter()
                .find(|(k, _)| k == "reason")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            OutboundCall::Decline {
                id: id.to_string(),
                reason,
            }
        } else if let Some(id) = rest.strip_suffix("/cancel") {
            OutboundCall::Cancel { id: id.to_string() }
        } else {
            OutboundCall::Create {
                username: rest.to_string(),
            }
        }
    }

    /// What one replayed event sequence produced: the ordered outbound calls and
    /// the number of game slots still held at the end (reserved or active).
    struct Replay {
        calls: Vec<OutboundCall>,
        active_slots: usize,
    }

    /// Drive `batches` of recorded NDJSON event lines through the event handler for
    /// a bot whose own id is `bot_id`, under `config`, against `client`'s recording
    /// transport, returning the outbound calls made and the slots still held.
    ///
    /// Each batch is one drain-and-process pass: every event in it is handled (a
    /// suitable challenge buffered, everything else acted on at once), then the
    /// accept queue is processed as a group. Splitting events across batches models
    /// how far apart they arrived — challenges in the same batch compete for slots
    /// together, while a `gameStart` in a later batch reflects Lichess answering an
    /// earlier accept. Matchmaking is disabled, so the only outbound calls are
    /// reactions to the replayed events.
    fn drive_batches(
        client: &LichessClient<FakeTransport>,
        bot_id: &str,
        config: &Config,
        batches: &[&[&str]],
    ) -> (Vec<OutboundCall>, usize) {
        let slots = GameSlots::new();
        let matchmaker = Mutex::new(Matchmaker::disabled());
        let shutdown = Shutdown::new();
        let mut pending = Vec::new();
        for batch in batches {
            for line in *batch {
                if let Some(event) = crate::event::parse_line(line).unwrap() {
                    handle_event(
                        client,
                        config,
                        bot_id,
                        &slots,
                        &matchmaker,
                        &shutdown,
                        &mut |_id: &str| {},
                        &mut pending,
                        event,
                    )
                    .unwrap();
                }
            }
            process_accept_queue(client, config, &slots, &mut pending).unwrap();
        }
        let calls = client_transport(client)
            .posts
            .borrow()
            .iter()
            .map(classify_post)
            .collect();
        (calls, slots.len())
    }

    /// Replay `batches` under a default recording transport and report the
    /// outbound calls and final slot count. For scenarios that need a transport
    /// configured (e.g. a 404 accept), build the client and call
    /// [`drive_batches`] directly.
    fn replay(bot_id: &str, config: &Config, batches: &[&[&str]]) -> Replay {
        let client = LichessClient::new(FakeTransport::new("{}", ""));
        let (calls, active_slots) = drive_batches(&client, bot_id, config, batches);
        Replay {
            calls,
            active_slots,
        }
    }

    // Captured Lichess challenge/game JSON in the shapes the live stream sends,
    // including fields the bot does not parse (direction, status, destUser, speed,
    // perf, color, finalColor, fullId, fen, source). Their presence confirms the
    // event types tolerate unknown fields.

    // The bot's own outgoing matchmaking challenge, echoed back on the account
    // stream. `direction` is "out" and the challenger is the bot itself.
    const SELF_OUTGOING_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"self01","direction":"out","status":"created","challenger":{"id":"seaborg","name":"seaborg","title":"BOT","rating":1800,"online":true},"destUser":{"id":"maia1","name":"maia1","title":"BOT","rating":1700},"variant":{"key":"standard","name":"Standard","short":"Std"},"rated":false,"speed":"blitz","timeControl":{"type":"clock","limit":300,"increment":3,"show":"5+3"},"color":"random","finalColor":"white","perf":{"icon":"","name":"Blitz"}}}"#;

    // A self challenge with NO `direction` field: the challenger id alone must
    // identify it as the bot's own.
    const SELF_CHALLENGE_NO_DIRECTION: &str = r#"{"type":"challenge","challenge":{"id":"self02","status":"created","challenger":{"id":"seaborg","name":"seaborg","title":"BOT","rating":1800},"destUser":{"id":"human1","name":"human1"},"variant":{"key":"standard"},"rated":false,"speed":"blitz","timeControl":{"type":"clock","limit":300,"increment":3},"color":"random","finalColor":"black"}}"#;

    // A genuine incoming human challenge that passes the default policy.
    const INCOMING_HUMAN_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"human99","direction":"in","status":"created","challenger":{"id":"alice","name":"alice","rating":1500,"title":null,"online":true},"destUser":{"id":"seaborg","name":"seaborg","title":"BOT"},"variant":{"key":"standard","name":"Standard"},"rated":false,"speed":"blitz","timeControl":{"type":"clock","limit":300,"increment":3,"show":"5+3"},"color":"random","finalColor":"white","perf":{"icon":"","name":"Blitz"}}}"#;

    // A genuine incoming Chess960 challenge — declined by the default policy.
    const INCOMING_VARIANT_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"var01","direction":"in","status":"created","challenger":{"id":"bob","name":"bob","rating":1600},"destUser":{"id":"seaborg","name":"seaborg"},"variant":{"key":"chess960","name":"Chess960"},"rated":false,"speed":"blitz","timeControl":{"type":"clock","limit":300,"increment":3},"color":"random"}}"#;

    // A genuine incoming challenge from another BOT account.
    const INCOMING_BOT_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"botchal","direction":"in","status":"created","challenger":{"id":"maia1","name":"maia1","rating":1700,"title":"BOT","online":true},"destUser":{"id":"seaborg","name":"seaborg","title":"BOT"},"variant":{"key":"standard","name":"Standard"},"rated":false,"speed":"blitz","timeControl":{"type":"clock","limit":300,"increment":3,"show":"5+3"},"color":"random"}}"#;

    // The challengeCanceled event for the incoming human challenge, sent when the
    // challenger withdraws it before it becomes a game. Carries the full challenge
    // object Lichess sends, of which only the id is modeled.
    const HUMAN_CHALLENGE_CANCELED: &str = r#"{"type":"challengeCanceled","challenge":{"id":"human99","status":"canceled","challenger":{"id":"alice","name":"alice","rating":1500},"destUser":{"id":"seaborg","name":"seaborg","title":"BOT"},"variant":{"key":"standard"},"rated":false,"timeControl":{"type":"clock","limit":300,"increment":3}}}"#;

    // The gameStart Lichess sends once the incoming human challenge is accepted.
    // Its game id is the challenge id (Lichess reuses the id), which is what lets
    // the accepted challenge's reserved slot reconcile with this start instead of
    // counting twice.
    const HUMAN_GAME_START: &str = r#"{"type":"gameStart","game":{"id":"human99","fullId":"human99abcd","color":"white","fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1","speed":"blitz","source":"friend","status":{"id":20,"name":"started"}}}"#;

    #[test]
    fn replay_harness_pins_challenge_lifecycle_scenarios() {
        // Each row is a sequence of event batches and the exact outbound calls plus
        // final held-slot count it must produce. Using real Lichess JSON (with
        // fields the bot does not parse) keeps these honest about the wire format
        // and exercises unknown-field tolerance.
        struct Scenario {
            name: &'static str,
            batches: &'static [&'static [&'static str]],
            expected_calls: Vec<OutboundCall>,
            expected_slots: usize,
        }

        let scenarios = vec![
            // (a) A self/outgoing challenge echoed on the stream is ignored: no
            // accept and no decline.
            Scenario {
                name: "self outgoing challenge is ignored",
                batches: &[&[SELF_OUTGOING_CHALLENGE]],
                expected_calls: vec![],
                expected_slots: 0,
            },
            // (c) A self challenge with `direction` absent is still ignored,
            // identified purely by the challenger id.
            Scenario {
                name: "self challenge without direction is ignored",
                batches: &[&[SELF_CHALLENGE_NO_DIRECTION]],
                expected_calls: vec![],
                expected_slots: 0,
            },
            // (b) An incoming human challenge that passes policy is accepted once;
            // its later gameStart reconciles the reserved slot into one active game.
            Scenario {
                name: "incoming human challenge is accepted and starts one game",
                batches: &[&[INCOMING_HUMAN_CHALLENGE], &[HUMAN_GAME_START]],
                expected_calls: vec![OutboundCall::Accept {
                    id: "human99".to_string(),
                }],
                expected_slots: 1,
            },
            // A genuine incoming challenge the policy rejects is still declined
            // with the right reason — the from_self guard does not suppress it.
            Scenario {
                name: "incoming variant challenge is declined",
                batches: &[&[INCOMING_VARIANT_CHALLENGE]],
                expected_calls: vec![OutboundCall::Decline {
                    id: "var01".to_string(),
                    reason: "variant".to_string(),
                }],
                expected_slots: 0,
            },
            // The bot's own echoed challenge and a real incoming one in the same
            // batch: the self one is skipped, the real one accepted.
            Scenario {
                name: "self echo is skipped while a real challenge is accepted",
                batches: &[
                    &[SELF_OUTGOING_CHALLENGE, INCOMING_HUMAN_CHALLENGE],
                    &[HUMAN_GAME_START],
                ],
                expected_calls: vec![OutboundCall::Accept {
                    id: "human99".to_string(),
                }],
                expected_slots: 1,
            },
        ];

        for scenario in scenarios {
            let result = replay(SELF_ID, &Config::default(), scenario.batches);
            assert_eq!(
                result.calls, scenario.expected_calls,
                "outbound calls mismatch in scenario: {}",
                scenario.name
            );
            assert_eq!(
                result.active_slots, scenario.expected_slots,
                "held slot count mismatch in scenario: {}",
                scenario.name
            );
        }
    }

    #[test]
    fn accept_reserves_a_slot_so_the_next_challenge_is_over_cap() {
        // The first human challenge is accepted and holds a slot while awaiting its
        // gameStart. A second incoming challenge arriving before that game starts
        // sees the cap already full and is declined for capacity, rather than
        // accepted into a slot that does not exist.
        let (calls, slots) = drive_batches(
            &LichessClient::new(FakeTransport::new("{}", "")),
            SELF_ID,
            &Config::default(),
            &[&[INCOMING_HUMAN_CHALLENGE], &[ACCEPTABLE_CHALLENGE]],
        );
        assert_eq!(
            calls,
            vec![
                OutboundCall::Accept {
                    id: "human99".to_string(),
                },
                OutboundCall::Decline {
                    id: "good01".to_string(),
                    reason: "generic".to_string(),
                },
            ]
        );
        // Only the first challenge's reservation remains.
        assert_eq!(slots, 1);
    }

    #[test]
    fn challenge_canceled_releases_the_reserved_slot() {
        // A challenge is accepted (reserving a slot), then withdrawn before its
        // game starts. The cancellation frees the reservation, leaving no slot held.
        let result = replay(
            SELF_ID,
            &Config::default(),
            &[&[INCOMING_HUMAN_CHALLENGE], &[HUMAN_CHALLENGE_CANCELED]],
        );
        assert_eq!(
            result.calls,
            vec![OutboundCall::Accept {
                id: "human99".to_string(),
            }]
        );
        assert_eq!(result.active_slots, 0);
    }

    #[test]
    fn a_404_accept_is_benign_and_frees_the_slot() {
        // The challenge vanishes (canceled or expired) between the decision and the
        // accept, so the accept POST answers 404. That is the spec's challenge-gone
        // outcome: it must not surface as an error and must free the reserved slot.
        let mut transport = FakeTransport::new("{}", "");
        transport.accept_not_found.insert("human99".to_string());
        let client = LichessClient::new(transport);
        // drive_batches unwraps every handler result, so reaching the assertions at
        // all proves the 404 did not surface as an error.
        let (calls, slots) = drive_batches(
            &client,
            SELF_ID,
            &Config::default(),
            &[&[INCOMING_HUMAN_CHALLENGE]],
        );
        // The accept was attempted, but its reserved slot was released on the 404.
        assert_eq!(
            calls,
            vec![OutboundCall::Accept {
                id: "human99".to_string(),
            }]
        );
        assert_eq!(slots, 0);
    }

    #[test]
    fn a_human_is_accepted_ahead_of_a_bot_in_the_same_burst() {
        use crate::config::ChallengePolicy;

        // A bot and a human challenge arrive together with only one slot. With human
        // preference on, the human is accepted first and the bot — now over the
        // single-slot cap — is declined, even though the bot arrived first.
        let config = Config {
            challenge: ChallengePolicy {
                accept_bots: true,
                prefer_human_challenges: true,
                ..ChallengePolicy::default()
            },
            max_concurrent_games: 1,
            ..Config::default()
        };
        let result = replay(
            SELF_ID,
            &config,
            &[&[INCOMING_BOT_CHALLENGE, INCOMING_HUMAN_CHALLENGE]],
        );
        assert_eq!(
            result.calls,
            vec![
                OutboundCall::Accept {
                    id: "human99".to_string(),
                },
                OutboundCall::Decline {
                    id: "botchal".to_string(),
                    reason: "generic".to_string(),
                },
            ]
        );
        assert_eq!(result.active_slots, 1);
    }

    #[test]
    fn a_bot_is_held_out_of_a_reserved_human_slot() {
        use crate::config::{ChallengePolicy, MatchmakingConfig};

        // One slot, reserved for humans. A bot challenge is declined for capacity —
        // the reserved slot is off-limits to it — while a human challenge is still
        // accepted into that same slot. Matchmaking is disabled, so reserving the
        // whole cap is allowed here.
        let config = Config {
            challenge: ChallengePolicy {
                accept_bots: true,
                ..ChallengePolicy::default()
            },
            matchmaking: MatchmakingConfig {
                reserved_human_slots: 1,
                ..MatchmakingConfig::default()
            },
            max_concurrent_games: 1,
            ..Config::default()
        };
        let result = replay(
            SELF_ID,
            &config,
            &[&[INCOMING_BOT_CHALLENGE], &[INCOMING_HUMAN_CHALLENGE]],
        );
        assert_eq!(
            result.calls,
            vec![
                OutboundCall::Decline {
                    id: "botchal".to_string(),
                    reason: "generic".to_string(),
                },
                OutboundCall::Accept {
                    id: "human99".to_string(),
                },
            ]
        );
        // The human holds the reserved slot; the bot took none.
        assert_eq!(result.active_slots, 1);
    }

    #[test]
    fn handling_accepts_and_declines_by_policy() {
        // A blank keepalive, an acceptable challenge, an unhandled event type,
        // a declinable challenge, and a game lifecycle, all in one stream.
        let stream = format!(
            "\n{ACCEPTABLE_CHALLENGE}\n{{\"type\":\"someFutureEvent\"}}\n{VARIANT_CHALLENGE}\n{{\"type\":\"gameStart\",\"game\":{{\"id\":\"g1\"}}}}\n{{\"type\":\"gameFinish\",\"game\":{{\"id\":\"g1\"}}}}\n"
        );
        let transport = FakeTransport::new("{}", &stream);
        let client = LichessClient::new(transport);
        let started = handle_one_stream(&client, SELF_ID);

        // The one started game is handed to the runner.
        assert_eq!(started, vec!["g1".to_string()]);

        let posts = client_transport(&client).posts.borrow().clone();
        assert_eq!(
            posts,
            vec![
                ("/api/challenge/good01/accept".to_string(), Vec::new()),
                (
                    "/api/challenge/bad960/decline".to_string(),
                    vec![("reason".to_string(), "variant".to_string())],
                ),
            ]
        );
    }

    #[test]
    fn handling_declines_when_at_game_cap() {
        // One game already running fills the default single-game cap, so the
        // following challenge is declined even though the policy would allow it.
        let stream = format!(
            "{{\"type\":\"gameStart\",\"game\":{{\"id\":\"g1\"}}}}\n{ACCEPTABLE_CHALLENGE}\n"
        );
        let transport = FakeTransport::new("{}", &stream);
        let client = LichessClient::new(transport);
        let started = handle_one_stream(&client, SELF_ID);

        // The game that filled the cap was still handed to the runner; only the
        // challenge that would have exceeded the cap is declined.
        assert_eq!(started, vec!["g1".to_string()]);
        assert_eq!(
            client_transport(&client).post_paths(),
            vec!["/api/challenge/good01/decline".to_string()]
        );
    }

    #[test]
    fn matchmaking_issues_a_challenge_to_an_eligible_idle_bot() {
        use crate::config::{MatchmakingConfig, MatchmakingMode};

        let mut transport = FakeTransport::new("{}", "");
        transport.bots_json =
            r#"{"id":"maia","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#.to_string();
        let client = LichessClient::new(transport);

        // Zero idle timeout so the very first seek is due, but a long interval so
        // the pending challenge from that first seek blocks the second — exactly
        // one challenge should be issued. The pools compose a 5+0 (blitz) casual
        // challenge.
        let config = MatchmakingConfig {
            enabled: true,
            variants: vec!["standard".to_string()],
            initial_seconds: vec![300],
            increment_seconds: vec![0],
            mode: MatchmakingMode::Casual,
            idle_timeout_seconds: 0,
            min_challenge_interval_seconds: 3600,
            ..MatchmakingConfig::default()
        };
        let matchmaker = Mutex::new(Matchmaker::new(config, 1, "me", Instant::now()));
        let slots = GameSlots::new();
        seek_times(&client, &slots, &matchmaker, 2);

        // Exactly one challenge was issued, to the eligible bot. (A second was not
        // stacked, because the first is now a pending challenge.)
        assert_eq!(
            client_transport(&client).post_paths(),
            vec!["/api/challenge/maia".to_string()]
        );
    }

    #[test]
    fn a_failed_challenge_moves_matchmaking_to_a_different_bot() {
        use crate::config::{MatchmakingConfig, MatchmakingMode};

        // The first bot's challenge is rejected at creation; without a penalty the
        // deterministic first-eligible selection would re-pick it on the second
        // seek. Instead the second seek must target the other bot.
        let mut transport = FakeTransport::new("{}", "");
        transport.bots_json = concat!(
            r#"{"id":"firstbot","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#,
            "\n",
            r#"{"id":"secondbot","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#,
        )
        .to_string();
        transport.challenge_create_fails = true;
        let client = LichessClient::new(transport);

        // Zero idle timeout and zero interval so both seeks fire. A 5+0 (blitz)
        // casual challenge matches both bots' ratings.
        let config = MatchmakingConfig {
            enabled: true,
            variants: vec!["standard".to_string()],
            initial_seconds: vec![300],
            increment_seconds: vec![0],
            mode: MatchmakingMode::Casual,
            idle_timeout_seconds: 0,
            min_challenge_interval_seconds: 0,
            ..MatchmakingConfig::default()
        };
        let matchmaker = Mutex::new(Matchmaker::new(config, 1, "me", Instant::now()));
        let slots = GameSlots::new();
        seek_times(&client, &slots, &matchmaker, 2);

        // Both attempts were made, and the second went to a different bot rather
        // than re-challenging the one that just failed.
        assert_eq!(
            client_transport(&client).post_paths(),
            vec![
                "/api/challenge/firstbot".to_string(),
                "/api/challenge/secondbot".to_string(),
            ]
        );
    }

    #[test]
    fn disabled_matchmaking_issues_no_challenge() {
        // A disabled matchmaker must produce no outgoing request at all: reactive
        // behaviour is unchanged.
        let mut transport = FakeTransport::new("{}", "");
        transport.bots_json =
            r#"{"id":"maia","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#.to_string();
        let client = LichessClient::new(transport);
        let slots = GameSlots::new();
        let matchmaker = Mutex::new(Matchmaker::disabled());
        seek_times(&client, &slots, &matchmaker, 1);
        assert!(client_transport(&client).post_paths().is_empty());
    }

    #[test]
    fn duplicate_game_start_does_not_spawn_a_second_worker() {
        // The event stream replays an in-progress game after reconnecting, so a
        // repeated gameStart for the same id must start the game only once.
        let stream = concat!(
            r#"{"type":"gameStart","game":{"id":"g1"}}"#,
            "\n",
            r#"{"type":"gameStart","game":{"id":"g1"}}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new("{}", stream));
        assert_eq!(handle_one_stream(&client, SELF_ID), vec!["g1".to_string()]);
    }

    #[test]
    fn game_slots_reserve_start_and_free() {
        let slots = GameSlots::new();

        // A fresh reservation counts; reserving the same id again does not.
        assert!(slots.reserve("g1"));
        assert!(
            !slots.reserve("g1"),
            "an id already held must not be reserved again"
        );
        assert_eq!(slots.len(), 1);

        // Starting the reserved game promotes it in place — still one slot — and
        // asks for a worker. Starting it again (a replayed gameStart) does not.
        assert!(slots.start("g1"), "the reserved game should spawn a worker");
        assert_eq!(slots.len(), 1, "promotion must not add a second slot");
        assert!(
            !slots.start("g1"),
            "a game already running must not spawn a second worker"
        );

        // A game the bot never reserved (an accepted outgoing challenge) is
        // recorded fresh on start and asks for a worker.
        assert!(slots.start("g2"));
        assert_eq!(slots.len(), 2);

        // Releasing a reservation only frees a still-reserved slot, never a
        // running game.
        slots.reserve("g3");
        slots.release_reservation("g3");
        assert_eq!(slots.len(), 2, "the reserved slot was freed");
        slots.release_reservation("g1");
        assert_eq!(slots.len(), 2, "a running game keeps its slot");

        // A worker removing its game frees the slot for the cap, and removing an
        // absent id (a duplicate removal) is harmless.
        slots.remove("g1");
        assert_eq!(slots.len(), 1);
        slots.remove("absent");
        assert_eq!(slots.len(), 1);
    }

    #[test]
    fn reader_reconnects_after_a_drop_then_stops_on_shutdown() {
        // Two connections, each ending (a drop). The injected reconnect wait
        // requests shutdown on its second call, so the reader reconnects once,
        // reads the second stream, then exits cleanly — forwarding both streams'
        // events in order.
        let first = format!("{ACCEPTABLE_CHALLENGE}\n");
        let second = format!("{VARIANT_CHALLENGE}\n");
        let client = LichessClient::new(FakeTransport::with_streams(
            "{}",
            [first.as_str(), second.as_str()],
        ));
        let shutdown = Shutdown::new();
        let waits = RefCell::new(0u32);
        let mut events = Vec::new();
        run_event_reader(
            &client,
            &shutdown,
            |event| {
                events.push(event);
                true
            },
            |_wait| {
                *waits.borrow_mut() += 1;
                if *waits.borrow() >= 2 {
                    shutdown.request();
                }
            },
        )
        .unwrap();

        // Both recorded streams were opened (one reconnect happened).
        assert_eq!(client_transport(&client).streams_remaining(), 0);
        assert_eq!(waits.into_inner(), 2);
        // Each connection's challenge was forwarded, in order.
        let ids: Vec<String> = events
            .into_iter()
            .filter_map(|event| match event {
                Event::Challenge { challenge } => Some(challenge.id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["good01".to_string(), "bad960".to_string()]);
    }

    #[test]
    fn reader_returns_immediately_when_already_shut_down() {
        // Shutdown requested before the reader starts: it returns without opening
        // a connection or forwarding anything.
        let client = LichessClient::new(FakeTransport::new("{}", ACCEPTABLE_CHALLENGE));
        let shutdown = Shutdown::new();
        shutdown.request();
        let mut events = Vec::new();
        run_event_reader(
            &client,
            &shutdown,
            |event| {
                events.push(event);
                true
            },
            |_wait| panic!("no reconnect wait during shutdown"),
        )
        .unwrap();
        assert!(events.is_empty());
        assert_eq!(
            client_transport(&client).streams_remaining(),
            1,
            "the stream must not be opened once shutdown is requested"
        );
    }

    /// A thread-safe [`Transport`] for the ingestion-isolation test. It delivers a
    /// single recorded event-stream connection (the recorded lines, then blank
    /// keepalives until shutdown), blocks the matchmaking challenge-create POST on
    /// a gate the test releases (standing in for a multi-minute `429` backoff),
    /// and signals the test when an accept is recorded and when the create begins
    /// blocking.
    struct ConcurrentTransport {
        bots_json: String,
        stream_lines: Mutex<Option<Vec<String>>>,
        shutdown: Shutdown,
        posts: Mutex<Vec<String>>,
        accept_tx: Mutex<Sender<String>>,
        create_started_tx: Mutex<Sender<()>>,
        create_gate: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Transport for ConcurrentTransport {
        fn get(&self, path: &str) -> Result<String> {
            if path.starts_with("/api/bot/online") {
                return Ok(self.bots_json.clone());
            }
            Ok("{}".to_string())
        }

        fn post_empty(&self, path: &str) -> Result<String> {
            self.posts.lock().unwrap().push(path.to_string());
            if path.ends_with("/accept") {
                let _ = self.accept_tx.lock().unwrap().send(path.to_string());
            }
            Ok(String::new())
        }

        fn post_form(&self, path: &str, _form: &[(&str, &str)]) -> Result<String> {
            self.posts.lock().unwrap().push(path.to_string());
            let is_create = path.starts_with("/api/challenge/")
                && !path.ends_with("/accept")
                && !path.ends_with("/decline");
            if is_create {
                // Announce that the outgoing challenge began, then block until the
                // test releases the gate — the stand-in for a long rate-limit
                // backoff pausing this thread.
                let _ = self.create_started_tx.lock().unwrap().send(());
                let (lock, cvar) = &*self.create_gate;
                let mut released = lock.lock().unwrap();
                while !*released {
                    released = cvar.wait(released).unwrap();
                }
            }
            Ok(String::new())
        }

        fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
            assert_eq!(path, "/api/stream/event", "unexpected stream path in test");
            let lines = self
                .stream_lines
                .lock()
                .unwrap()
                .take()
                .expect("event stream opened more than once");
            Ok(Box::new(KeepaliveStream {
                lines: lines.into_iter(),
                shutdown: self.shutdown.clone(),
            }))
        }
    }

    /// The recorded lines, followed by blank keepalives (with a short pause each)
    /// until shutdown ends the connection — the way a live stream stays open
    /// between events and only drops when the connection dies.
    struct KeepaliveStream {
        lines: std::vec::IntoIter<String>,
        shutdown: Shutdown,
    }

    impl Iterator for KeepaliveStream {
        type Item = Result<String>;

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(line) = self.lines.next() {
                return Some(Ok(line));
            }
            if self.shutdown.is_requested() {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
            Some(Ok(String::new()))
        }
    }

    #[test]
    fn incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked() {
        use crate::config::{MatchmakingConfig, MatchmakingMode};

        let (accept_tx, accept_rx) = mpsc::channel::<String>();
        let (create_started_tx, create_started_rx) = mpsc::channel::<()>();
        let create_gate = Arc::new((Mutex::new(false), Condvar::new()));
        let shutdown = Shutdown::new();

        // The stream delivers one incoming human challenge, then keepalives.
        let transport = ConcurrentTransport {
            bots_json: r#"{"id":"maia","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#
                .to_string(),
            stream_lines: Mutex::new(Some(vec![ACCEPTABLE_CHALLENGE.to_string()])),
            shutdown: shutdown.clone(),
            posts: Mutex::new(Vec::new()),
            accept_tx: Mutex::new(accept_tx),
            create_started_tx: Mutex::new(create_started_tx),
            create_gate: Arc::clone(&create_gate),
        };
        let client = Arc::new(LichessClient::new(transport));

        // Matchmaking enabled and immediately due (zero idle, zero interval), so
        // the matchmaking thread issues an outgoing challenge at once — which the
        // transport blocks, standing in for a 429 backoff.
        let config = MatchmakingConfig {
            enabled: true,
            variants: vec!["standard".to_string()],
            initial_seconds: vec![300],
            increment_seconds: vec![0],
            mode: MatchmakingMode::Casual,
            idle_timeout_seconds: 0,
            min_challenge_interval_seconds: 0,
            ..MatchmakingConfig::default()
        };
        let matchmaker = Arc::new(Mutex::new(Matchmaker::new(config, 1, "me", Instant::now())));
        let slots = GameSlots::new();
        let (events_tx, events_rx) = mpsc::channel::<Event>();

        let reader = {
            let client = Arc::clone(&client);
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                run_event_reader(
                    &client,
                    &shutdown,
                    |event| events_tx.send(event).is_ok(),
                    |wait| shutdown.sleep(wait),
                )
                .unwrap();
            })
        };
        let matchmaker_thread = {
            let client = Arc::clone(&client);
            let slots = slots.clone();
            let matchmaker = Arc::clone(&matchmaker);
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                run_matchmaking(&client, &slots, &matchmaker, &shutdown, |wait| {
                    shutdown.sleep(wait)
                })
                .unwrap();
            })
        };
        let consumer = {
            let client = Arc::clone(&client);
            let slots = slots.clone();
            let matchmaker = Arc::clone(&matchmaker);
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                run_event_consumer(
                    &client,
                    &Config::default(),
                    SELF_ID,
                    &slots,
                    &matchmaker,
                    &shutdown,
                    events_rx,
                    |_id: &str| {},
                )
                .unwrap();
            })
        };

        // The matchmaking challenge-create has started and is now blocked.
        create_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("matchmaking issued an outgoing challenge");
        // While that call is blocked, the incoming challenge is still accepted
        // promptly — the property the single-threaded design failed to hold.
        let accepted = accept_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("incoming challenge accepted while matchmaking is blocked");
        assert_eq!(accepted, "/api/challenge/good01/accept");

        // Release the blocked matchmaking call and shut down. Every thread stops
        // promptly (no waiting out a full backoff), proving shutdown stays
        // responsive.
        {
            let (lock, cvar) = &*create_gate;
            *lock.lock().unwrap() = true;
            cvar.notify_all();
        }
        shutdown.request();
        reader.join().unwrap();
        matchmaker_thread.join().unwrap();
        consumer.join().unwrap();
    }

    #[test]
    fn non_bot_account_is_rejected_on_startup() {
        let account = r#"{"id":"human","username":"Human","title":null}"#;
        let transport = FakeTransport::new(account, "");
        let client = LichessClient::new(transport);
        match require_bot_account(&client) {
            Err(Error::NotBotAccount { username }) => assert_eq!(username, "Human"),
            other => panic!("expected NotBotAccount, got {other:?}"),
        }
    }

    #[test]
    fn upgrade_confirmed_posts_upgrade() {
        let account = r#"{"id":"fresh","username":"Fresh","title":null,"count":{"all":0}}"#;
        let transport = FakeTransport::new(account, "");
        let client = LichessClient::new(transport);
        let outcome = upgrade_account(&client, |_| true).unwrap();
        assert_eq!(outcome, UpgradeOutcome::Upgraded);
        assert_eq!(
            client_transport(&client).post_paths(),
            vec!["/api/bot/account/upgrade".to_string()]
        );
    }

    #[test]
    fn upgrade_declined_makes_no_request() {
        let account = r#"{"id":"fresh","username":"Fresh","title":null,"count":{"all":0}}"#;
        let transport = FakeTransport::new(account, "");
        let client = LichessClient::new(transport);
        let outcome = upgrade_account(&client, |_| false).unwrap();
        assert_eq!(outcome, UpgradeOutcome::Cancelled);
        assert!(client_transport(&client).post_paths().is_empty());
    }

    #[test]
    fn upgrade_already_bot_is_noop() {
        let account = r#"{"id":"bot","username":"Bot","title":"BOT","count":{"all":0}}"#;
        let transport = FakeTransport::new(account, "");
        let client = LichessClient::new(transport);
        // The confirmation must never run for an account that is already a bot.
        let outcome = upgrade_account(&client, |_| panic!("should not confirm")).unwrap();
        assert_eq!(outcome, UpgradeOutcome::AlreadyBot);
        assert!(client_transport(&client).post_paths().is_empty());
    }

    #[test]
    fn upgrade_with_games_is_ineligible() {
        let account = r#"{"id":"played","username":"Played","title":null,"count":{"all":7}}"#;
        let transport = FakeTransport::new(account, "");
        let client = LichessClient::new(transport);
        match upgrade_account(&client, |_| true) {
            Err(Error::UpgradeIneligible { username, games }) => {
                assert_eq!(username, "Played");
                assert_eq!(games, 7);
            }
            other => panic!("expected UpgradeIneligible, got {other:?}"),
        }
    }

    /// A trivial [`Transport`] that returns empty responses and never streams. It
    /// holds no interior mutability, so it is `Send + Sync` and can back a client
    /// shared with the consumer thread — unlike [`FakeTransport`]. The drain tests
    /// send no events, so its methods are never actually exercised.
    struct SilentTransport;

    impl Transport for SilentTransport {
        fn get(&self, _path: &str) -> Result<String> {
            Ok("{}".to_string())
        }

        fn post_empty(&self, _path: &str) -> Result<String> {
            Ok(String::new())
        }

        fn post_form(&self, _path: &str, _form: &[(&str, &str)]) -> Result<String> {
            Ok(String::new())
        }

        fn open_stream(&self, _path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
            Ok(Box::new(std::iter::empty()))
        }
    }

    /// Spawn [`run_event_consumer`] on its own thread with a disabled matchmaker,
    /// returning the event sender (kept alive by the caller so the channel does not
    /// disconnect) and the join handle carrying the consumer's result. The slots
    /// and shutdown handles are shared, so a test can mutate them and observe the
    /// consumer react on its next poll.
    fn spawn_consumer(
        client: Arc<LichessClient<SilentTransport>>,
        slots: GameSlots,
        shutdown: Shutdown,
    ) -> (Sender<Event>, std::thread::JoinHandle<Result<()>>) {
        let (events_tx, events_rx) = mpsc::channel::<Event>();
        let handle = std::thread::spawn(move || {
            let matchmaker = Mutex::new(Matchmaker::disabled());
            run_event_consumer(
                &client,
                &Config::default(),
                SELF_ID,
                &slots,
                &matchmaker,
                &shutdown,
                events_rx,
                |_id: &str| {},
            )
        });
        (events_tx, handle)
    }

    #[test]
    fn draining_with_a_game_still_running_keeps_the_consumer_alive_until_it_ends() {
        // The core drain-to-zero transition: one game is in flight when the first
        // interrupt arrives. Draining must not stop the consumer — the game plays
        // on — and only once its worker frees the slot does the consumer escalate
        // to an immediate shutdown and return, a clean exit with no forfeit.
        let client = Arc::new(LichessClient::new(SilentTransport));
        let slots = GameSlots::new();
        assert!(slots.start("g1"), "seed one in-flight game");
        let shutdown = Shutdown::new();
        let (_events_tx, handle) =
            spawn_consumer(Arc::clone(&client), slots.clone(), shutdown.clone());

        // First interrupt: enter drain. The consumer keeps running because a game
        // is still active, so it does not join yet.
        assert!(shutdown.begin_drain());
        std::thread::sleep(CONSUMER_POLL * 3);
        assert!(
            !handle.is_finished(),
            "the consumer must keep running while a game is in flight"
        );
        assert!(
            !shutdown.is_requested(),
            "draining alone must not request an immediate shutdown"
        );

        // The game's worker finishes and frees its slot. Draining now sees zero
        // active games and shuts down cleanly on its own.
        slots.remove("g1");
        let result = handle.join().expect("consumer thread panicked");
        assert!(result.is_ok(), "the consumer exits cleanly: {result:?}");
        assert!(
            shutdown.is_requested(),
            "reaching zero active games while draining escalates to shutdown"
        );
    }

    #[test]
    fn a_second_interrupt_while_draining_shuts_down_immediately() {
        // Normal -> drain -> immediate shutdown. A game is in flight, so drain does
        // not exit on its own; a second interrupt (an explicit shutdown request)
        // must end the consumer at once, without waiting for the game to finish.
        let client = Arc::new(LichessClient::new(SilentTransport));
        let slots = GameSlots::new();
        assert!(slots.start("g1"), "seed one in-flight game");
        let shutdown = Shutdown::new();
        let (_events_tx, handle) =
            spawn_consumer(Arc::clone(&client), slots.clone(), shutdown.clone());

        assert!(shutdown.begin_drain());
        std::thread::sleep(CONSUMER_POLL * 2);
        assert!(
            !handle.is_finished(),
            "a still-running game keeps the draining consumer alive"
        );

        // Second interrupt: immediate shutdown while the game is still active.
        shutdown.request();
        let result = handle.join().expect("consumer thread panicked");
        assert!(result.is_ok(), "the consumer returns cleanly: {result:?}");
        assert_eq!(
            slots.len(),
            1,
            "the consumer does not touch the in-flight slot; its worker resigns it"
        );
    }

    #[test]
    fn draining_declines_an_incoming_challenge_instead_of_accepting_it() {
        // While draining, a fresh incoming challenge the policy would normally
        // accept must be declined rather than started as a new game, and no slot
        // may be reserved for it.
        let client = LichessClient::new(FakeTransport::new("{}", ""));
        let slots = GameSlots::new();
        let matchmaker = Mutex::new(Matchmaker::disabled());
        let shutdown = Shutdown::new();
        assert!(shutdown.begin_drain());
        let mut pending = Vec::new();
        let event = crate::event::parse_line(INCOMING_HUMAN_CHALLENGE)
            .unwrap()
            .unwrap();
        handle_event(
            &client,
            &Config::default(),
            SELF_ID,
            &slots,
            &matchmaker,
            &shutdown,
            &mut |_id: &str| panic!("no game may start while draining"),
            &mut pending,
            event,
        )
        .unwrap();
        // The challenge was declined outright, none was buffered for acceptance,
        // and no slot was taken.
        let calls: Vec<OutboundCall> = client_transport(&client)
            .posts
            .borrow()
            .iter()
            .map(classify_post)
            .collect();
        assert_eq!(
            calls,
            vec![OutboundCall::Decline {
                id: "human99".to_string(),
                reason: "generic".to_string(),
            }]
        );
        assert!(pending.is_empty(), "a draining bot buffers no challenge");
        assert_eq!(slots.len(), 0, "no slot is reserved while draining");
    }

    #[test]
    fn draining_matchmaker_seeks_no_new_game() {
        use crate::config::{MatchmakingConfig, MatchmakingMode};

        // An eligible idle bot is available and matchmaking is immediately due, so
        // a running bot would challenge it. Draining must suppress the seek so no
        // outgoing challenge is issued.
        let mut transport = FakeTransport::new("{}", "");
        transport.bots_json =
            r#"{"id":"maia","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#.to_string();
        let client = LichessClient::new(transport);
        let config = MatchmakingConfig {
            enabled: true,
            variants: vec!["standard".to_string()],
            initial_seconds: vec![300],
            increment_seconds: vec![0],
            mode: MatchmakingMode::Casual,
            idle_timeout_seconds: 0,
            min_challenge_interval_seconds: 0,
            ..MatchmakingConfig::default()
        };
        let matchmaker = Mutex::new(Matchmaker::new(config, 1, "me", Instant::now()));
        let slots = GameSlots::new();
        let shutdown = Shutdown::new();
        assert!(shutdown.begin_drain());
        // Even though a seek would otherwise be due, draining makes it a no-op.
        seek_matchmaking_game(&client, &slots, &matchmaker, &shutdown).unwrap();
        assert!(
            client_transport(&client).post_paths().is_empty(),
            "a draining matchmaker issues no challenge"
        );
    }

    #[test]
    fn drain_message_states_the_count_and_the_second_interrupt() {
        // The operator message must name how many games remain and make clear a
        // second interrupt quits immediately, so an operator knows what draining
        // is doing and how to escalate.
        let message = drain_message(3);
        assert!(message.contains('3'), "states the remaining game count");
        assert!(
            message.contains("again") && message.contains("immediately"),
            "tells the operator a second interrupt quits now: {message}"
        );
    }

    /// Borrow the transport back out of a client for assertions.
    fn client_transport<T: Transport>(client: &LichessClient<T>) -> &T {
        client.transport()
    }
}
