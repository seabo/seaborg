//! Playing an accepted game to completion.
//!
//! When a game starts, a worker calls [`play_game`], which opens the game's
//! stream, keeps a [`Position`] in step with the moves the server reports, and
//! on the bot's turn computes a reply and submits it. The move itself comes from
//! a [`MoveChooser`]: production wraps the search engine in [`EngineMoveChooser`],
//! while tests substitute a deterministic picker so the loop can be exercised
//! against recorded streams without launching a search.

use std::time::Duration;

use chess::mov::Move;
use chess::position::{Player, Position};
use engine::search::{SearchEngine, SearchLimit};
use engine::time::TimeControl;

use crate::backoff::{Backoff, RECONNECT_BASE, RECONNECT_MAX};
use crate::client::LichessClient;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::game_stream::{GameEvent, GameFull, GameState};
use crate::shutdown::Shutdown;
use crate::transport::Transport;

/// Chooses the move to play in a position under a time budget.
///
/// `None` means the position has no legal move — the bot is checkmated or
/// stalemated — so there is nothing to submit and the server's terminal state
/// will follow.
pub trait MoveChooser {
    fn choose(&self, position: &Position, limit: SearchLimit) -> Option<Move>;
}

/// A [`MoveChooser`] backed by the search engine.
///
/// The transposition table lives for the whole game, so successive searches
/// reuse each other's work rather than starting cold every move.
pub struct EngineMoveChooser {
    engine: SearchEngine,
}

impl EngineMoveChooser {
    /// Build a chooser whose engine uses a `hash_mb`-mebibyte hash table.
    pub fn new(hash_mb: usize) -> EngineMoveChooser {
        EngineMoveChooser {
            engine: SearchEngine::new(hash_mb),
        }
    }
}

impl MoveChooser for EngineMoveChooser {
    fn choose(&self, position: &Position, limit: SearchLimit) -> Option<Move> {
        self.engine
            .start(position.clone(), limit)
            .wait()
            .result()
            .and_then(|result| result.best_move)
    }
}

/// Play the game with the given id to completion, streaming its states and
/// replying with `chooser`'s moves on the bot's turn.
///
/// `bot_id` is the bot's own Lichess account id, matched against the game's two
/// players to find which side the bot has. Game streams drop routinely, so a
/// disconnect that is not a terminal game-over reconnects with exponential
/// backoff; because the position is rebuilt from the server's authoritative move
/// list on every state, a reconnect resumes exactly in sync. When `shutdown` is
/// requested the in-flight game is resigned cleanly rather than left mid-move.
pub fn play_game<T, C>(
    client: &LichessClient<T>,
    config: &Config,
    bot_id: &str,
    game_id: &str,
    chooser: &C,
    shutdown: &Shutdown,
) -> Result<()>
where
    T: Transport,
    C: MoveChooser,
{
    play_game_reconnecting(client, config, bot_id, game_id, chooser, shutdown, |wait| {
        shutdown.sleep(wait)
    })
}

/// The reconnect loop behind [`play_game`], with the reconnect wait injected so
/// tests can drive it without real sleeps.
fn play_game_reconnecting<T, C, S>(
    client: &LichessClient<T>,
    config: &Config,
    bot_id: &str,
    game_id: &str,
    chooser: &C,
    shutdown: &Shutdown,
    mut sleep: S,
) -> Result<()>
where
    T: Transport,
    C: MoveChooser,
    S: FnMut(Duration),
{
    let mut backoff = Backoff::new(RECONNECT_BASE, RECONNECT_MAX);
    loop {
        if shutdown.is_requested() {
            return resign_on_shutdown(client, game_id);
        }
        match play_game_once(client, config, bot_id, game_id, chooser, shutdown)? {
            GameOutcome::Finished => return Ok(()),
            GameOutcome::ShutdownRequested => return resign_on_shutdown(client, game_id),
            GameOutcome::Disconnected { made_progress } => {
                if shutdown.is_requested() {
                    return resign_on_shutdown(client, game_id);
                }
                // A connection that carried real game data before dropping is
                // treated as healthy, so its next unrelated drop starts backing
                // off from the base delay again.
                if made_progress {
                    backoff.reset();
                }
                log::warn!("game {game_id}: stream disconnected; reconnecting");
                sleep(backoff.next_delay());
            }
        }
    }
}

/// Resign an in-flight game as part of shutdown, best effort.
///
/// A failure to reach the resign endpoint must not mask the shutdown itself, so
/// it is logged and swallowed; the process is going down regardless.
fn resign_on_shutdown<T: Transport>(client: &LichessClient<T>, game_id: &str) -> Result<()> {
    log::info!("game {game_id}: resigning on shutdown");
    if let Err(error) = client.resign_game(game_id) {
        log::warn!("game {game_id}: resign request failed during shutdown: {error}");
    }
    Ok(())
}

/// Why a single game-stream connection stopped.
#[derive(Debug)]
enum GameOutcome {
    /// The server reported a terminal status: the game is genuinely over.
    Finished,
    /// Shutdown was requested while the game was still in progress.
    ShutdownRequested,
    /// The connection ended without a terminal status. `made_progress` is whether
    /// any real game data arrived before it dropped.
    Disconnected { made_progress: bool },
}

/// Stream one game-stream connection to a stopping point.
///
/// Returns [`GameOutcome::Finished`] on a terminal status, `ShutdownRequested`
/// when the shutdown flag trips mid-stream, and `Disconnected` when the stream
/// ends or hits a recoverable transport error. A non-recoverable error (a
/// rejected token, or a malformed/illegal move list) propagates.
fn play_game_once<T, C>(
    client: &LichessClient<T>,
    config: &Config,
    bot_id: &str,
    game_id: &str,
    chooser: &C,
    shutdown: &Shutdown,
) -> Result<GameOutcome>
where
    T: Transport,
    C: MoveChooser,
{
    let stream = match client.game_stream(game_id) {
        Ok(stream) => stream,
        Err(error) if error.is_recoverable() => {
            return Ok(GameOutcome::Disconnected {
                made_progress: false,
            })
        }
        Err(error) => return Err(error),
    };

    let mut game: Option<GameContext<'_, T, C>> = None;
    let mut made_progress = false;

    for item in stream {
        if shutdown.is_requested() {
            return Ok(GameOutcome::ShutdownRequested);
        }
        let event = match item {
            Ok(Some(event)) => event,
            // Keepalive line: no event, but a chance to notice shutdown, which
            // the check at the top of the next iteration takes.
            Ok(None) => continue,
            Err(error) if error.is_recoverable() => {
                return Ok(GameOutcome::Disconnected { made_progress })
            }
            Err(error) => return Err(error),
        };
        made_progress = true;

        match event {
            GameEvent::GameFull(full) => {
                let context = GameContext::new(client, config, chooser, game_id, bot_id, &full)?;
                // The opening message already carries a state; the bot may be on
                // move immediately, having the white pieces. On a reconnect this
                // gameFull rebuilds the context and resyncs from the move list.
                if let Flow::Stop = context.on_state(&full.state, shutdown)? {
                    return Ok(GameOutcome::Finished);
                }
                game = Some(context);
            }
            GameEvent::GameState(state) => {
                let context = game.as_ref().ok_or_else(|| {
                    Error::Decode(format!("game {game_id}: gameState arrived before gameFull"))
                })?;
                if let Flow::Stop = context.on_state(&state, shutdown)? {
                    return Ok(GameOutcome::Finished);
                }
            }
            // Chat and opponent-gone notifications carry no move; claiming a win
            // when the opponent leaves is left to later hardening.
            GameEvent::ChatLine(_) | GameEvent::OpponentGone(_) | GameEvent::Other => {}
        }
    }

    Ok(GameOutcome::Disconnected { made_progress })
}

/// Whether the game loop should keep reading the stream after handling a state.
enum Flow {
    Continue,
    Stop,
}

/// The unchanging facts of one game, plus the collaborators a state update needs.
struct GameContext<'a, T: Transport, C: MoveChooser> {
    client: &'a LichessClient<T>,
    chooser: &'a C,
    game_id: &'a str,
    /// The side the bot is playing.
    our_side: Player,
    /// The position the game starts from, before any moves.
    base: Position,
    /// Time held back from our clock before allocating a move, in milliseconds.
    move_overhead_ms: u32,
}

impl<'a, T: Transport, C: MoveChooser> GameContext<'a, T, C> {
    fn new(
        client: &'a LichessClient<T>,
        config: &Config,
        chooser: &'a C,
        game_id: &'a str,
        bot_id: &str,
        full: &GameFull,
    ) -> Result<GameContext<'a, T, C>> {
        let our_side = our_side(full, bot_id).ok_or_else(|| {
            Error::Decode(format!("game {game_id}: bot `{bot_id}` is neither player"))
        })?;
        let base = base_position(full)?;
        Ok(GameContext {
            client,
            chooser,
            game_id,
            our_side,
            base,
            move_overhead_ms: config.engine.move_overhead_ms,
        })
    }

    /// Handle one game state: stop on a terminal status, otherwise reconstruct
    /// the position and, if it is the bot's turn, compute and submit a move.
    fn on_state(&self, state: &GameState, shutdown: &Shutdown) -> Result<Flow> {
        if !state.is_ongoing() {
            log::info!("game {}: finished ({})", self.game_id, state.status);
            return Ok(Flow::Stop);
        }

        let position = replay(&self.base, &state.moves)?;
        if position.turn() != self.our_side {
            // The opponent is on move; wait for the next state.
            return Ok(Flow::Continue);
        }

        if shutdown.is_requested() {
            // It is our move, but the bot is shutting down: do not start a fresh
            // search or submit a move. The worker loop observes the shutdown next
            // and resigns the game cleanly.
            return Ok(Flow::Continue);
        }

        let limit = search_limit(state, &position, self.move_overhead_ms);
        match self.chooser.choose(&position, limit) {
            Some(mov) => {
                let uci = mov.to_uci_string();
                log::info!("game {}: playing {uci}", self.game_id);
                self.client.play_move(self.game_id, &uci)?;
            }
            None => {
                // No legal move: the bot is mated or stalemated. There is nothing
                // to send; the server's terminal state ends the loop next.
                log::info!("game {}: no legal move to play", self.game_id);
            }
        }
        Ok(Flow::Continue)
    }
}

/// Which side the bot has, by matching its account id against the two players.
///
/// Returns `None` if the bot is neither player, which should not happen for a
/// game the account was told it is in.
fn our_side(full: &GameFull, bot_id: &str) -> Option<Player> {
    if full.white.id.as_deref() == Some(bot_id) {
        Some(Player::WHITE)
    } else if full.black.id.as_deref() == Some(bot_id) {
        Some(Player::BLACK)
    } else {
        None
    }
}

/// The position a game starts from, before any moves are applied.
///
/// Standard games report `startpos` (or omit the field); a custom starting
/// position or a position-based variant reports a FEN instead.
fn base_position(full: &GameFull) -> Result<Position> {
    match full.initial_fen.as_deref() {
        None | Some("startpos") => Ok(Position::start_pos()),
        Some(fen) => Position::from_fen(fen)
            .map_err(|e| Error::Decode(format!("game {}: initial FEN {fen:?}: {e:?}", full.id))),
    }
}

/// Rebuild the current position by replaying the server's move list from `base`.
///
/// Rebuilding from the authoritative list every state, rather than tracking one
/// position incrementally, keeps the bot in step with the server even if a state
/// is missed, and turns any divergence into an explicit error here.
fn replay(base: &Position, moves: &str) -> Result<Position> {
    let mut position = base.clone();
    for uci in moves.split_whitespace() {
        if position.make_uci_move(uci).is_none() {
            return Err(Error::Decode(format!(
                "illegal move `{uci}` in game stream move list"
            )));
        }
    }
    Ok(position)
}

/// Derive the search budget for the bot's move from the clock the server sent.
///
/// The configured overhead is held back from the bot's clock before the time
/// manager slices it, so a move computed just under the clock still reaches
/// Lichess before the flag falls. Reducing the clock the manager sees (rather
/// than trimming its final allotment) keeps the allocation proportional at fast
/// controls, where a flat deduction would collapse the budget to nothing.
///
/// Lichess real-time games carry no periodic move count, so the manager is told
/// there is none and lets the increment fund the steady state.
fn search_limit(state: &GameState, position: &Position, move_overhead_ms: u32) -> SearchLimit {
    let margin = u64::from(move_overhead_ms);
    let control = TimeControl::new(
        state.wtime.saturating_sub(margin),
        state.btime.saturating_sub(margin),
        state.winc,
        state.binc,
        None,
    );
    let budget_ms = control.to_move_time(position.move_number(), position.turn());
    SearchLimit::Time(Duration::from_millis(budget_ms))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use chess::mono_traits::{All, Legal};
    use chess::movelist::BasicMoveList;

    use super::*;
    use crate::config::Config;
    use crate::shutdown::Shutdown;

    /// A [`MoveChooser`] that returns the first legal move, so the loop can be
    /// exercised deterministically without launching a search.
    struct FirstLegalMove;

    impl MoveChooser for FirstLegalMove {
        fn choose(&self, position: &Position, _limit: SearchLimit) -> Option<Move> {
            let moves = position.generate::<BasicMoveList, All, Legal>();
            (&moves).into_iter().next().copied()
        }
    }

    /// A [`Transport`] that replays one recorded game stream per connection (in
    /// order, so a reconnect after a drop is fed the next stream) and records the
    /// POSTs the bot makes, touching no network.
    struct FakeTransport {
        streams: RefCell<VecDeque<String>>,
        posts: RefCell<Vec<String>>,
    }

    impl FakeTransport {
        /// A transport that serves a single connection from `stream`.
        fn new(stream: &str) -> FakeTransport {
            FakeTransport::with_streams([stream])
        }

        /// A transport that serves one connection per recorded stream, in order.
        fn with_streams<'a>(streams: impl IntoIterator<Item = &'a str>) -> FakeTransport {
            FakeTransport {
                streams: RefCell::new(streams.into_iter().map(str::to_string).collect()),
                posts: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transport for FakeTransport {
        fn get(&self, path: &str) -> Result<String> {
            panic!("unexpected GET {path} in game test");
        }

        fn post_empty(&self, path: &str) -> Result<String> {
            self.posts.borrow_mut().push(path.to_string());
            Ok(String::new())
        }

        fn post_form(&self, path: &str, _form: &[(&str, &str)]) -> Result<String> {
            panic!("unexpected form POST {path} in game test");
        }

        fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
            assert!(
                path.starts_with("/api/bot/game/stream/"),
                "unexpected stream path {path}"
            );
            let stream = self
                .streams
                .borrow_mut()
                .pop_front()
                .expect("game test opened more connections than it recorded streams");
            let lines: Vec<Result<String>> = stream.lines().map(|l| Ok(l.to_string())).collect();
            Ok(Box::new(lines.into_iter()))
        }
    }

    /// The uci submitted in each recorded move POST, in order.
    fn submitted_moves(client: &LichessClient<FakeTransport>) -> Vec<String> {
        client
            .transport()
            .posts
            .borrow()
            .iter()
            .filter(|path| path.contains("/move/"))
            .map(|path| path.rsplit('/').next().unwrap().to_string())
            .collect()
    }

    /// Every recorded POST path, in order (moves and resigns).
    fn post_paths(client: &LichessClient<FakeTransport>) -> Vec<String> {
        client.transport().posts.borrow().clone()
    }

    // A Scholar's-mate game recorded with the bot playing black: white opens,
    // black replies each of its three turns, and white mates on move four.
    const SCHOLARS_MATE_BOT_BLACK: &str = concat!(
        r#"{"type":"gameFull","id":"sm1","white":{"id":"alice","name":"Alice"},"black":{"id":"seaborg","name":"seaborg"},"initialFen":"startpos","state":{"type":"gameState","moves":"","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4 e7e5","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4 e7e5 f1c4","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4 e7e5 f1c4 b8c6","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4 e7e5 f1c4 b8c6 d1h5","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4 e7e5 f1c4 b8c6 d1h5 g8f6","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#,
        "\n",
        r#"{"type":"gameState","moves":"e2e4 e7e5 f1c4 b8c6 d1h5 g8f6 h5f7","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"mate","winner":"white"}"#,
        "\n",
    );

    #[test]
    fn plays_a_legal_move_on_each_of_its_turns() {
        let transport = FakeTransport::new(SCHOLARS_MATE_BOT_BLACK);
        let client = LichessClient::new(transport);
        let outcome = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "sm1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap();
        assert!(matches!(outcome, GameOutcome::Finished));

        // Black is on move after white's first, third, and fifth plies: three
        // moves, and no move for the terminal `mate` state that follows.
        let submitted = submitted_moves(&client);
        assert_eq!(submitted.len(), 3, "expected one move per black turn");

        // Each submission is legal in the position rebuilt from the stream's
        // move list up to that turn, proving the bot stays in sync with the
        // server for the whole game.
        let black_turn_move_lists = ["e2e4", "e2e4 e7e5 f1c4", "e2e4 e7e5 f1c4 b8c6 d1h5"];
        for (uci, moves) in submitted.iter().zip(black_turn_move_lists) {
            let mut position = replay(&Position::start_pos(), moves).unwrap();
            assert!(
                position.make_uci_move(uci).is_some(),
                "submitted `{uci}` is not legal after `{moves}`"
            );
        }
    }

    #[test]
    fn plays_immediately_with_the_white_pieces() {
        // With the bot on white, the opening gameFull is already its turn, so it
        // must move without waiting for a further state.
        let stream = concat!(
            r#"{"type":"gameFull","id":"w1","white":{"id":"seaborg","name":"seaborg"},"black":{"id":"bob","name":"Bob"},"state":{"type":"gameState","moves":"","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
            r#"{"type":"gameState","moves":"e2e4 c7c5","wtime":59000,"btime":60000,"winc":0,"binc":0,"status":"resign","winner":"white"}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new(stream));
        let outcome = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "w1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap();
        assert!(matches!(outcome, GameOutcome::Finished));

        // One move for the opening position, then a stop on the resignation.
        let submitted = submitted_moves(&client);
        assert_eq!(submitted.len(), 1);
        assert!(Position::start_pos()
            .clone()
            .make_uci_move(&submitted[0])
            .is_some());
    }

    #[test]
    fn missing_initial_fen_starts_from_the_standard_position() {
        // No `initialFen` field at all: the game starts from the standard setup.
        let stream = concat!(
            r#"{"type":"gameFull","id":"n1","white":{"id":"seaborg","name":"seaborg"},"black":{"id":"bob","name":"Bob"},"state":{"type":"gameState","moves":"","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new(stream));
        // The stream ends without a terminal status, so a single connection ends
        // in a disconnect; the one move it managed is still recorded.
        let outcome = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "n1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap();
        assert!(matches!(
            outcome,
            GameOutcome::Disconnected {
                made_progress: true
            }
        ));
        assert_eq!(submitted_moves(&client).len(), 1);
    }

    #[test]
    fn ignores_chat_and_opponent_gone_without_moving() {
        // It is the opponent's move throughout, so these notifications must not
        // provoke a submission.
        let stream = concat!(
            r#"{"type":"gameFull","id":"c1","white":{"id":"alice","name":"Alice"},"black":{"id":"seaborg","name":"seaborg"},"state":{"type":"gameState","moves":"","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
            r#"{"type":"chatLine","username":"alice","text":"hello","room":"player"}"#,
            "\n",
            r#"{"type":"opponentGone","gone":false}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new(stream));
        play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "c1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap();
        assert!(submitted_moves(&client).is_empty());
    }

    #[test]
    fn game_state_before_game_full_is_an_error() {
        let stream = concat!(
            r#"{"type":"gameState","moves":"e2e4","wtime":1,"btime":1,"winc":0,"binc":0,"status":"started"}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new(stream));
        let err = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "x1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }

    #[test]
    fn bot_absent_from_players_is_an_error() {
        let stream = concat!(
            r#"{"type":"gameFull","id":"a1","white":{"id":"alice","name":"Alice"},"black":{"id":"bob","name":"Bob"},"state":{"type":"gameState","moves":"","wtime":1,"btime":1,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new(stream));
        let err = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "a1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }

    #[test]
    fn illegal_move_in_the_stream_is_an_error() {
        // `e2e5` is not a legal opening move, so replaying the list must fail
        // rather than silently desynchronize.
        let stream = concat!(
            r#"{"type":"gameFull","id":"i1","white":{"id":"alice","name":"Alice"},"black":{"id":"seaborg","name":"seaborg"},"state":{"type":"gameState","moves":"e2e5","wtime":1000,"btime":1000,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::new(stream));
        let err = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "i1",
            &FirstLegalMove,
            &Shutdown::new(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Decode(_)));

        // The illegal move list is a protocol fault, not a transient drop, so it
        // must surface rather than being retried by the reconnect loop.
        assert!(!err.is_recoverable());
    }

    #[test]
    fn reconnects_after_a_midgame_drop_and_finishes() {
        // First connection: the bot (black) replies to 1.e4, then the stream
        // drops with the game still in progress. Second connection: the stream
        // reopens and immediately reports a terminal status, ending the game.
        let first = concat!(
            r#"{"type":"gameFull","id":"r1","white":{"id":"alice","name":"Alice"},"black":{"id":"seaborg","name":"seaborg"},"state":{"type":"gameState","moves":"","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
            r#"{"type":"gameState","moves":"e2e4","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}"#,
            "\n",
        );
        let second = concat!(
            r#"{"type":"gameFull","id":"r1","white":{"id":"alice","name":"Alice"},"black":{"id":"seaborg","name":"seaborg"},"state":{"type":"gameState","moves":"e2e4","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"resign","winner":"black"}}"#,
            "\n",
        );
        let client = LichessClient::new(FakeTransport::with_streams([first, second]));
        let sleeps = RefCell::new(Vec::new());
        play_game_reconnecting(
            &client,
            &Config::default(),
            "seaborg",
            "r1",
            &FirstLegalMove,
            &Shutdown::new(),
            |wait| sleeps.borrow_mut().push(wait),
        )
        .unwrap();

        // Exactly one reconnect happened (one backoff wait between the two
        // connections), and the single move from before the drop was submitted.
        assert_eq!(sleeps.into_inner().len(), 1, "expected one reconnect wait");
        assert_eq!(submitted_moves(&client).len(), 1);
    }

    #[test]
    fn resigns_the_in_flight_game_on_shutdown() {
        // Shutdown is already requested, so the worker resigns before opening a
        // connection rather than playing on.
        let shutdown = Shutdown::new();
        shutdown.request();
        let client = LichessClient::new(FakeTransport::new(""));
        play_game_reconnecting(
            &client,
            &Config::default(),
            "seaborg",
            "g",
            &FirstLegalMove,
            &shutdown,
            |_| panic!("must not wait to reconnect during shutdown"),
        )
        .unwrap();
        assert_eq!(
            post_paths(&client),
            vec!["/api/bot/game/g/resign".to_string()]
        );
    }

    #[test]
    fn shutdown_midstream_stops_before_moving() {
        // The bot has white and is on move at the opening, but shutdown is set:
        // a single connection reports the shutdown and submits no move.
        let stream = concat!(
            r#"{"type":"gameFull","id":"s1","white":{"id":"seaborg","name":"seaborg"},"black":{"id":"bob","name":"Bob"},"state":{"type":"gameState","moves":"","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}}"#,
            "\n",
        );
        let shutdown = Shutdown::new();
        shutdown.request();
        let client = LichessClient::new(FakeTransport::new(stream));
        let outcome = play_game_once(
            &client,
            &Config::default(),
            "seaborg",
            "s1",
            &FirstLegalMove,
            &shutdown,
        )
        .unwrap();
        assert!(matches!(outcome, GameOutcome::ShutdownRequested));
        assert!(
            submitted_moves(&client).is_empty(),
            "no move may be submitted once shutdown is requested"
        );
    }

    #[test]
    fn search_budget_holds_back_the_overhead_and_stays_under_the_clock() {
        // A generous clock: the budget is positive, matches the time manager's
        // own figure for the overhead-reduced clock, and never exceeds what is
        // on the clock.
        let state = GameState {
            moves: String::new(),
            wtime: 300_000,
            btime: 300_000,
            winc: 3_000,
            binc: 3_000,
            status: "started".to_string(),
        };
        let position = Position::start_pos();

        let SearchLimit::Time(budget) = search_limit(&state, &position, 100) else {
            panic!("expected a time limit");
        };
        let expected = TimeControl::new(299_900, 299_900, 3_000, 3_000, None)
            .to_move_time(position.move_number(), position.turn());
        assert_eq!(budget, Duration::from_millis(expected));
        assert!(budget > Duration::ZERO);
        assert!(budget < Duration::from_millis(300_000));

        // A larger safety margin can only shrink the budget.
        let SearchLimit::Time(smaller) = search_limit(&state, &position, 5_000) else {
            panic!("expected a time limit");
        };
        assert!(smaller <= budget);
    }

    #[test]
    fn search_budget_saturates_rather_than_underflowing_on_a_tiny_clock() {
        // A clock below the safety margin must not underflow; the budget floors
        // at zero, under which the search still returns a legal move.
        let state = GameState {
            moves: String::new(),
            wtime: 40,
            btime: 40,
            winc: 0,
            binc: 0,
            status: "started".to_string(),
        };
        let SearchLimit::Time(budget) = search_limit(&state, &Position::start_pos(), 100) else {
            panic!("expected a time limit");
        };
        assert_eq!(budget, Duration::ZERO);
    }
}
