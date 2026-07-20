//! Top-level entry points that wire configuration, transport, and the event
//! loop together for the `seaborg lichess` command.

use std::path::Path;
use std::sync::Arc;

use crate::account::Account;
use crate::client::LichessClient;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::event::Event;
use crate::game::{play_game, EngineMoveChooser};
use crate::policy::{self, Decision};
use crate::transport::{HttpTransport, Transport};

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

/// Connect to Lichess and run the challenge-handling event loop until the stream
/// ends, playing each accepted game in its own worker thread.
///
/// Fails fast when the token is missing or rejected, or when the authenticated
/// account is not a BOT account (which needs [`upgrade`] first).
pub fn run(config_path: Option<&Path>) -> Result<()> {
    let token = load_token()?;
    let config = Arc::new(Config::load(config_path)?);
    let client = Arc::new(LichessClient::new(HttpTransport::new(
        crate::DEFAULT_BASE_URL,
        token,
    )));

    let account = require_bot_account(&client)?;
    log::info!("connected to Lichess as bot {}", account.username);
    // Lichess reports each game's players by their lowercase account id, which
    // is what identifies the bot's own side once a game starts.
    let bot_id = account.id;

    // Each accepted game runs to completion on its own thread, matching the
    // repo's std-thread idiom, so a slow search in one game cannot stall the
    // event loop or the other games. The event loop keeps the concurrency count
    // from the account stream's game lifecycle events (below).
    let start_game = {
        let client = Arc::clone(&client);
        let config = Arc::clone(&config);
        move |game_id: &str| {
            let client = Arc::clone(&client);
            let config = Arc::clone(&config);
            let bot_id = bot_id.clone();
            let game_id = game_id.to_string();
            std::thread::spawn(move || {
                let chooser = EngineMoveChooser::new(config.engine.hash_mb);
                if let Err(error) = play_game(&client, &config, &bot_id, &game_id, &chooser) {
                    log::warn!("game {game_id} stopped on error: {error}");
                }
            });
        }
    };

    run_event_loop(&client, &config, start_game)
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
    let client = LichessClient::new(HttpTransport::new(crate::DEFAULT_BASE_URL, token));
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

/// Read the event stream and act on each event: accept or decline challenges by
/// policy, and track how many games are in progress so the game cap is honored.
///
/// `start_game` is invoked with a game's id when it starts, to begin playing it.
/// The active-game count that gates the cap is kept from the account stream's
/// own `gameStart`/`gameFinish` events, which are the authoritative lifecycle
/// signal, rather than from the game workers.
///
/// Generic over the transport so it can be driven by recorded NDJSON in tests.
pub fn run_event_loop<T, S>(
    client: &LichessClient<T>,
    config: &Config,
    mut start_game: S,
) -> Result<()>
where
    T: Transport,
    S: FnMut(&str),
{
    let mut active_games: u32 = 0;

    for event in client.event_stream()? {
        match event? {
            Event::Challenge { challenge } => {
                let decision = policy::evaluate(
                    &challenge,
                    &config.challenge,
                    active_games,
                    config.max_concurrent_games,
                );
                match decision {
                    Decision::Accept => {
                        log::info!(
                            "accepting challenge {} from {}",
                            challenge.id,
                            challenge.challenger.name
                        );
                        client.accept_challenge(&challenge.id)?;
                    }
                    Decision::Decline(reason) => {
                        log::info!(
                            "declining challenge {} from {} ({})",
                            challenge.id,
                            challenge.challenger.name,
                            reason.as_str()
                        );
                        client.decline_challenge(&challenge.id, reason)?;
                    }
                }
            }
            Event::GameStart { game } => {
                active_games = active_games.saturating_add(1);
                log::info!(
                    "game {} started ({}/{} active)",
                    game.id,
                    active_games,
                    config.max_concurrent_games
                );
                start_game(&game.id);
            }
            Event::GameFinish { game } => {
                active_games = active_games.saturating_sub(1);
                log::info!("game {} finished ({} active)", game.id, active_games);
            }
            Event::Other => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::config::Config;
    use crate::transport::Transport;

    /// A recorded POST: the request path and its form fields (empty for bodiless
    /// posts).
    type RecordedPost = (String, Vec<(String, String)>);

    /// A [`Transport`] that replays a recorded event stream and a canned account
    /// response, and records the POSTs the bot makes. It never touches the
    /// network, so challenge handling can be asserted deterministically.
    struct FakeTransport {
        account_json: String,
        stream: String,
        posts: RefCell<Vec<RecordedPost>>,
    }

    impl FakeTransport {
        fn new(account_json: &str, stream: &str) -> FakeTransport {
            FakeTransport {
                account_json: account_json.to_string(),
                stream: stream.to_string(),
                posts: RefCell::new(Vec::new()),
            }
        }

        /// The request paths POSTed, in order.
        fn post_paths(&self) -> Vec<String> {
            self.posts.borrow().iter().map(|(p, _)| p.clone()).collect()
        }
    }

    impl Transport for FakeTransport {
        fn get(&self, path: &str) -> Result<String> {
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
            Ok(String::new())
        }

        fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
            assert_eq!(path, "/api/stream/event", "unexpected stream path in test");
            // `str::lines` yields empty strings for the blank keepalive lines,
            // exercising the loop's tolerance of them.
            let lines: Vec<Result<String>> = self
                .stream
                .lines()
                .map(|line| Ok(line.to_string()))
                .collect();
            Ok(Box::new(lines.into_iter()))
        }
    }

    // A standard, casual 5+3 challenge from a human — accepted by the defaults.
    const ACCEPTABLE_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"good01","rated":false,"variant":{"key":"standard"},"timeControl":{"type":"clock","limit":300,"increment":3},"challenger":{"id":"alice","name":"alice","rating":1500,"title":null}}}"#;

    // A Chess960 challenge — declined by the default standard-only policy.
    const VARIANT_CHALLENGE: &str = r#"{"type":"challenge","challenge":{"id":"bad960","rated":false,"variant":{"key":"chess960"},"timeControl":{"type":"clock","limit":300,"increment":3},"challenger":{"id":"bob","name":"bob","rating":1600,"title":null}}}"#;

    #[test]
    fn event_loop_accepts_and_declines_by_policy() {
        // A blank keepalive, an acceptable challenge, an unhandled event type,
        // a declinable challenge, and a game lifecycle, all in one stream.
        let stream = format!(
            "\n{ACCEPTABLE_CHALLENGE}\n{{\"type\":\"challengeCanceled\"}}\n{VARIANT_CHALLENGE}\n{{\"type\":\"gameStart\",\"game\":{{\"id\":\"g1\"}}}}\n{{\"type\":\"gameFinish\",\"game\":{{\"id\":\"g1\"}}}}\n"
        );
        let transport = FakeTransport::new("{}", &stream);
        let client = LichessClient::new(transport);
        let mut started = Vec::new();
        run_event_loop(&client, &Config::default(), |id: &str| {
            started.push(id.to_string())
        })
        .unwrap();

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
        let mut started = Vec::new();
        run_event_loop(&client, &Config::default(), |id: &str| {
            started.push(id.to_string())
        })
        .unwrap();

        // The game that filled the cap was still handed to the runner; only the
        // challenge that would have exceeded the cap is declined.
        assert_eq!(started, vec!["g1".to_string()]);
        assert_eq!(
            client_transport(&client).post_paths(),
            vec!["/api/challenge/good01/decline".to_string()]
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
