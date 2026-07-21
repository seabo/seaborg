//! Top-level entry points that wire configuration, transport, and the event
//! loop together for the `seaborg lichess` command.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::account::Account;
use crate::backoff::{Backoff, RECONNECT_BASE, RECONNECT_MAX};
use crate::client::LichessClient;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::event::Event;
use crate::game::{play_game, EngineMoveChooser};
use crate::matchmaking::{Action, Matchmaker};
use crate::policy::{self, Decision};
use crate::shutdown::{self, Shutdown};
use crate::transport::{HttpTransport, Transport};

/// How many online bots to fetch when looking for a matchmaking opponent. A small
/// page is enough: the bot only needs one eligible opponent, and a fresh page is
/// fetched on each attempt.
const ONLINE_BOTS_LIMIT: u32 = 50;

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
/// The event stream and per-game streams drop routinely; both reconnect with
/// exponential backoff rather than ending the bot. On Ctrl-C the bot stops
/// accepting new challenges and waits for its in-flight games to resign and exit
/// cleanly instead of dropping their connections mid-move.
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
    // is what identifies the bot's own side once a game starts.
    let bot_id = account.id;

    // Proactive matchmaking. Disabled by default, in which case the loop is
    // purely reactive; enabling it lets the bot challenge other bots when idle.
    let mut matchmaker = Matchmaker::new(
        config.matchmaking.clone(),
        config.max_concurrent_games,
        bot_id.clone(),
        Instant::now(),
    );
    if matchmaker.is_enabled() {
        log::info!("matchmaking enabled: will challenge idle bots");
    }

    // Each accepted game runs to completion on its own thread, matching the
    // repo's std-thread idiom, so a slow search in one game cannot stall the
    // event loop or the other games. The handles are kept so shutdown can wait
    // for every worker to resign and exit rather than dropping mid-move.
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();

    // Which games have a live worker. This persists across event-stream
    // reconnects, so a `gameStart` replayed on reconnect does not spawn a second
    // worker for a game already in progress, and it is the source of truth for
    // the concurrency cap.
    let active = ActiveGames::new();

    let spawn_game = |game_id: &str| -> std::thread::JoinHandle<()> {
        let client = Arc::clone(&client);
        let config = Arc::clone(&config);
        let shutdown = shutdown.clone();
        let active = active.clone();
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
            active.remove(&game_id);
        })
    };

    let result = run_event_loop(
        &client,
        &config,
        &shutdown,
        &active,
        &mut matchmaker,
        |game_id| workers.push(spawn_game(game_id)),
        |wait| shutdown.sleep(wait),
    );

    // However the loop ended, wind the workers down: request shutdown so any
    // in-flight game resigns, then join every worker before returning so the
    // process does not exit while a game is still connected.
    shutdown.request();
    for worker in workers {
        let _ = worker.join();
    }
    result
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

/// The set of games that currently have a live worker.
///
/// Shared between the event loop — which records a game when it starts and reads
/// the count for the concurrency cap — and each game worker, which removes its
/// own game when it exits. Worker-driven removal keeps the cap correct even if a
/// `gameFinish` event is missed while the event stream is disconnected, which the
/// event-driven count alone could not guarantee.
#[derive(Clone, Default)]
pub struct ActiveGames(Arc<Mutex<HashSet<String>>>);

impl ActiveGames {
    /// An empty set.
    pub fn new() -> ActiveGames {
        ActiveGames::default()
    }

    /// Record `id` as active, returning whether it was newly inserted. A `false`
    /// means a worker already tracks this game, so the caller must not start
    /// another for it.
    fn insert(&self, id: &str) -> bool {
        self.0.lock().unwrap().insert(id.to_string())
    }

    /// Drop `id` from the set. Idempotent, so the worker and a `gameFinish` event
    /// removing the same game is harmless.
    fn remove(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }

    /// How many games currently have a worker.
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

/// Read the account event stream and act on each event, reconnecting with
/// exponential backoff when the stream drops.
///
/// `start_game` is invoked with a game's id the first time that game starts, to
/// begin playing it. `active` tracks the games with live workers; it gates the
/// concurrency cap and survives reconnects so a replayed `gameStart` never spawns
/// a duplicate worker. `sleep` performs the reconnect wait (injected so tests can
/// avoid real delays). The loop returns cleanly once shutdown is requested.
///
/// Generic over the transport so it can be driven by recorded NDJSON in tests.
pub fn run_event_loop<T, S, P>(
    client: &LichessClient<T>,
    config: &Config,
    shutdown: &Shutdown,
    active: &ActiveGames,
    matchmaker: &mut Matchmaker,
    mut start_game: S,
    mut sleep: P,
) -> Result<()>
where
    T: Transport,
    S: FnMut(&str),
    P: FnMut(Duration),
{
    let mut backoff = Backoff::new(RECONNECT_BASE, RECONNECT_MAX);
    loop {
        if shutdown.is_requested() {
            return Ok(());
        }
        match run_event_stream_once(
            client,
            config,
            shutdown,
            active,
            matchmaker,
            &mut start_game,
        )? {
            StreamOutcome::Shutdown => return Ok(()),
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
    /// Shutdown was requested; the loop should end.
    Shutdown,
    /// The connection ended without a fatal error. `made_progress` is whether any
    /// event arrived before it dropped.
    Disconnected { made_progress: bool },
}

/// Consume one event-stream connection until it drops, a fatal error occurs, or
/// shutdown is requested.
fn run_event_stream_once<T, S>(
    client: &LichessClient<T>,
    config: &Config,
    shutdown: &Shutdown,
    active: &ActiveGames,
    matchmaker: &mut Matchmaker,
    start_game: &mut S,
) -> Result<StreamOutcome>
where
    T: Transport,
    S: FnMut(&str),
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
            return Ok(StreamOutcome::Shutdown);
        }
        match item {
            Ok(Some(event)) => {
                made_progress = true;
                handle_event(client, config, active, matchmaker, start_game, event)?;
            }
            // Keepalive line: no event to handle, but a regular chance to seek a
            // matchmaking game and (via the check above) to notice shutdown.
            Ok(None) => {}
            Err(error) if error.is_recoverable() => {
                return Ok(StreamOutcome::Disconnected { made_progress })
            }
            Err(error) => return Err(error),
        }
        // Each event and each keepalive is a moment to consider seeking a game;
        // when matchmaking is disabled this is a cheap no-op.
        maybe_seek_matchmaking_game(client, active, matchmaker)?;
    }
    Ok(StreamOutcome::Disconnected { made_progress })
}

/// If matchmaking is due, fetch online bots, pick an eligible opponent, and issue
/// a challenge.
///
/// Does nothing unless matchmaking is enabled and the [`Matchmaker`] judges the
/// bot idle enough to seek a game. A transient failure to list bots or issue the
/// challenge is logged and swallowed so one bad request does not end the bot; a
/// non-recoverable error still surfaces.
fn maybe_seek_matchmaking_game<T: Transport>(
    client: &LichessClient<T>,
    active: &ActiveGames,
    matchmaker: &mut Matchmaker,
) -> Result<()> {
    if !matchmaker.is_enabled() {
        return Ok(());
    }
    let now = Instant::now();
    if matchmaker.choose(now, active.len() as u32) != Action::Seek {
        return Ok(());
    }
    // Count this as an attempt up front so a failed lookup or an empty candidate
    // list still waits out the minimum interval before retrying.
    matchmaker.record_attempt(now);

    let bots = match client.online_bots(ONLINE_BOTS_LIMIT) {
        Ok(bots) => bots,
        Err(error) if error.is_recoverable() => {
            log::warn!("listing online bots for matchmaking: {error}");
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    let spec = matchmaker.compose_spec();
    let target = matchmaker
        .select_opponent(&spec, &bots, now)
        .map(|bot| bot.id.clone());
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
        Ok(()) => matchmaker.record_issued(now),
        Err(error) if error.is_recoverable() => {
            log::warn!("challenging bot {target}: {error}");
            // The challenge did not take (commonly a creation-time rejection).
            // Back off from this bot so the deterministic first-eligible selection
            // does not re-pick it every interval and wedge matchmaking on one
            // unreachable opponent.
            matchmaker.record_challenge_failed(&target, now);
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Act on one account event: accept or decline a challenge by policy, or track a
/// game's lifecycle. A transient failure to accept or decline a challenge is
/// logged and swallowed so one bad request does not end the bot; a non-recoverable
/// error (a rejected token) still surfaces.
fn handle_event<T, S>(
    client: &LichessClient<T>,
    config: &Config,
    active: &ActiveGames,
    matchmaker: &mut Matchmaker,
    start_game: &mut S,
    event: Event,
) -> Result<()>
where
    T: Transport,
    S: FnMut(&str),
{
    match event {
        Event::Challenge { challenge } => {
            let decision = policy::evaluate(
                &challenge,
                &config.challenge,
                active.len() as u32,
                config.max_concurrent_games,
            );
            match decision {
                Decision::Accept => {
                    log::info!(
                        "accepting challenge {} from {}",
                        challenge.id,
                        challenge.challenger.name
                    );
                    tolerate_recoverable(client.accept_challenge(&challenge.id), || {
                        format!("accepting challenge {}", challenge.id)
                    })?;
                }
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
            matchmaker.record_game_started(Instant::now());
            if active.insert(&game.id) {
                log::info!(
                    "game {} started ({}/{} active)",
                    game.id,
                    active.len(),
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
            active.remove(&game.id);
            log::info!("game {} finished ({} active)", game.id, active.len());
        }
        Event::ChallengeDeclined { challenge } => {
            // A bot we challenged declined. Record it so matchmaking backs off
            // from re-challenging that bot for the configured window.
            if let Some(dest) = challenge.dest_user {
                log::info!("bot {} declined our challenge", dest.id);
                matchmaker.record_declined(&dest.id, Instant::now());
            }
        }
        Event::Other => {}
    }
    Ok(())
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
    use std::collections::VecDeque;

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

    /// Drive one event-stream connection to its end, returning the game ids the
    /// runner was asked to start. Used by the challenge-handling tests, which
    /// assert on a single connection without the reconnect wrapper.
    fn drive_one_connection(client: &LichessClient<FakeTransport>) -> Vec<String> {
        let active = ActiveGames::new();
        let mut started = Vec::new();
        run_event_stream_once(
            client,
            &Config::default(),
            &Shutdown::new(),
            &active,
            &mut Matchmaker::disabled(),
            &mut |id: &str| started.push(id.to_string()),
        )
        .unwrap();
        started
    }

    #[test]
    fn event_loop_accepts_and_declines_by_policy() {
        // A blank keepalive, an acceptable challenge, an unhandled event type,
        // a declinable challenge, and a game lifecycle, all in one stream.
        let stream = format!(
            "\n{ACCEPTABLE_CHALLENGE}\n{{\"type\":\"challengeCanceled\"}}\n{VARIANT_CHALLENGE}\n{{\"type\":\"gameStart\",\"game\":{{\"id\":\"g1\"}}}}\n{{\"type\":\"gameFinish\",\"game\":{{\"id\":\"g1\"}}}}\n"
        );
        let transport = FakeTransport::new("{}", &stream);
        let client = LichessClient::new(transport);
        let started = drive_one_connection(&client);

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
    fn event_loop_declines_when_at_game_cap() {
        // One game already running fills the default single-game cap, so the
        // following challenge is declined even though the policy would allow it.
        let stream = format!(
            "{{\"type\":\"gameStart\",\"game\":{{\"id\":\"g1\"}}}}\n{ACCEPTABLE_CHALLENGE}\n"
        );
        let transport = FakeTransport::new("{}", &stream);
        let client = LichessClient::new(transport);
        let started = drive_one_connection(&client);

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

        // A stream of just keepalives: no incoming events, so the only thing that
        // can happen is a matchmaking tick issuing an outgoing challenge.
        let mut transport = FakeTransport::new("{}", "\n\n");
        transport.bots_json =
            r#"{"id":"maia","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#.to_string();
        let client = LichessClient::new(transport);

        // Zero idle timeout so the very first keepalive is due to seek, but a long
        // interval so the pending challenge from that first tick blocks the second
        // keepalive's tick — exactly one challenge should be issued. The pools
        // compose a 5+0 (blitz) casual challenge.
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
        let mut matchmaker = Matchmaker::new(config, 1, "me", Instant::now());
        let active = ActiveGames::new();
        run_event_stream_once(
            &client,
            &Config::default(),
            &Shutdown::new(),
            &active,
            &mut matchmaker,
            &mut |_id: &str| {},
        )
        .unwrap();

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

        // Two keepalives drive two seek ticks. The first bot's challenge is
        // rejected at creation; without a penalty the deterministic first-eligible
        // selection would re-pick it on the second tick. Instead the second tick
        // must target the other bot.
        let mut transport = FakeTransport::new("{}", "\n\n");
        transport.bots_json = concat!(
            r#"{"id":"firstbot","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#,
            "\n",
            r#"{"id":"secondbot","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#,
        )
        .to_string();
        transport.challenge_create_fails = true;
        let client = LichessClient::new(transport);

        // Zero idle timeout and zero interval so both keepalive ticks seek. A 5+0
        // (blitz) casual challenge matches both bots' ratings.
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
        let mut matchmaker = Matchmaker::new(config, 1, "me", Instant::now());
        let active = ActiveGames::new();
        run_event_stream_once(
            &client,
            &Config::default(),
            &Shutdown::new(),
            &active,
            &mut matchmaker,
            &mut |_id: &str| {},
        )
        .unwrap();

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
    fn disabled_matchmaking_issues_no_challenge_on_a_keepalive() {
        // The same keepalive-only stream with matchmaking off must produce no
        // outgoing request at all: reactive behaviour is unchanged.
        let mut transport = FakeTransport::new("{}", "\n\n");
        transport.bots_json =
            r#"{"id":"maia","title":"BOT","perfs":{"blitz":{"rating":1600}}}"#.to_string();
        let client = LichessClient::new(transport);
        let active = ActiveGames::new();
        run_event_stream_once(
            &client,
            &Config::default(),
            &Shutdown::new(),
            &active,
            &mut Matchmaker::disabled(),
            &mut |_id: &str| {},
        )
        .unwrap();
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
        assert_eq!(drive_one_connection(&client), vec!["g1".to_string()]);
    }

    #[test]
    fn active_games_tracks_membership_and_frees_slots() {
        let active = ActiveGames::new();
        assert!(active.insert("g1"));
        assert!(
            !active.insert("g1"),
            "a game already tracked must not be inserted again"
        );
        assert!(active.insert("g2"));
        assert_eq!(active.len(), 2);

        // A worker removing its game frees the slot for the cap.
        active.remove("g1");
        assert_eq!(active.len(), 1);
        // Removing a game that is not present (e.g. a duplicate removal) is safe.
        active.remove("absent");
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn event_loop_reconnects_after_a_drop_then_stops_on_shutdown() {
        // Two connections, each ending (a drop). The injected reconnect wait
        // requests shutdown on its second call, so the loop reconnects once, runs
        // the second stream, then exits cleanly.
        let first = format!("{ACCEPTABLE_CHALLENGE}\n");
        let second = format!("{VARIANT_CHALLENGE}\n");
        let client = LichessClient::new(FakeTransport::with_streams(
            "{}",
            [first.as_str(), second.as_str()],
        ));
        let shutdown = Shutdown::new();
        let active = ActiveGames::new();
        let waits = RefCell::new(0u32);
        run_event_loop(
            &client,
            &Config::default(),
            &shutdown,
            &active,
            &mut Matchmaker::disabled(),
            |_id| {},
            |_wait| {
                *waits.borrow_mut() += 1;
                if *waits.borrow() >= 2 {
                    shutdown.request();
                }
            },
        )
        .unwrap();

        // Both recorded streams were opened (one reconnect happened), and each
        // connection's challenge was acted on.
        assert_eq!(client_transport(&client).streams_remaining(), 0);
        assert_eq!(waits.into_inner(), 2);
        assert_eq!(
            client_transport(&client).post_paths(),
            vec![
                "/api/challenge/good01/accept".to_string(),
                "/api/challenge/bad960/decline".to_string(),
            ]
        );
    }

    #[test]
    fn event_loop_returns_immediately_when_already_shut_down() {
        // Shutdown requested before the loop starts: it returns without opening a
        // connection or accepting anything.
        let client = LichessClient::new(FakeTransport::new("{}", ACCEPTABLE_CHALLENGE));
        let shutdown = Shutdown::new();
        shutdown.request();
        let active = ActiveGames::new();
        run_event_loop(
            &client,
            &Config::default(),
            &shutdown,
            &active,
            &mut Matchmaker::disabled(),
            |_id| panic!("no game should start during shutdown"),
            |_wait| panic!("no reconnect wait during shutdown"),
        )
        .unwrap();
        assert!(client_transport(&client).post_paths().is_empty());
        assert_eq!(
            client_transport(&client).streams_remaining(),
            1,
            "the stream must not be opened once shutdown is requested"
        );
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

    /// Borrow the transport back out of a client for assertions.
    fn client_transport<T: Transport>(client: &LichessClient<T>) -> &T {
        client.transport()
    }
}
