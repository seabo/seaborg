//! Game-stream types and NDJSON parsing.
//!
//! The bot game stream (`GET /api/bot/game/stream/{gameId}`) emits one JSON
//! object per line: a single `gameFull` describing the whole game, then a
//! `gameState` after every move, interleaved with `chatLine` and `opponentGone`
//! notifications. These types model only the fields the game runner acts on;
//! unknown event types and unknown object fields are ignored so a Lichess API
//! addition does not break the stream.

use serde::Deserialize;

use crate::error::{Error, Result};

/// One event from a game's stream.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameEvent {
    /// The opening message: the full game, including its initial state.
    GameFull(GameFull),
    /// A state update sent after each move (and on clock/termination changes).
    GameState(GameState),
    /// A chat message in the player or spectator room.
    ChatLine(ChatLine),
    /// A notification that the opponent left or returned.
    OpponentGone(OpponentGone),
    /// Any other event type, kept so the stream tolerates messages the runner
    /// does not act on.
    #[serde(other)]
    Other,
}

/// The opening `gameFull` message describing a whole game.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFull {
    /// The game's Lichess id.
    pub id: String,
    /// The player with the white pieces.
    pub white: GamePlayer,
    /// The player with the black pieces.
    pub black: GamePlayer,
    /// The position the game starts from. `startpos` (or absence) means the
    /// standard starting position; otherwise it is a FEN. Only ever set for
    /// games begun from a custom position or a position-based variant.
    #[serde(default)]
    pub initial_fen: Option<String>,
    /// The current game state, carrying the move list and clocks.
    pub state: GameState,
}

/// A player in a game.
///
/// A human or bot opponent carries an `id`; an AI opponent carries only its
/// level and no id, so `id` is optional and identifies which side is the bot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayer {
    /// The player's account id, absent for the Lichess AI.
    #[serde(default)]
    pub id: Option<String>,
}

/// A `gameState` update.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    /// All moves played so far as space-separated UCI strings, empty before the
    /// first move.
    #[serde(default)]
    pub moves: String,
    /// White's remaining clock in milliseconds.
    #[serde(default)]
    pub wtime: u64,
    /// Black's remaining clock in milliseconds.
    #[serde(default)]
    pub btime: u64,
    /// White's per-move increment in milliseconds.
    #[serde(default)]
    pub winc: u64,
    /// Black's per-move increment in milliseconds.
    #[serde(default)]
    pub binc: u64,
    /// The game status (`started`, `mate`, `resign`, `outoftime`, ...). Any
    /// value other than an in-progress one marks the game as over.
    pub status: String,
}

impl GameState {
    /// Whether the game is still in progress under this status.
    ///
    /// Lichess uses `created` before the first move and `started` while play is
    /// under way; every other status is terminal (mate, resignation, timeout,
    /// draw, abort, and so on).
    pub fn is_ongoing(&self) -> bool {
        matches!(self.status.as_str(), "created" | "started")
    }
}

/// A chat message carried on the game stream.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLine {
    /// The sender's username.
    #[serde(default)]
    pub username: String,
    /// The message text.
    #[serde(default)]
    pub text: String,
    /// The room the message was sent to (`player` or `spectator`).
    #[serde(default)]
    pub room: String,
}

/// An `opponentGone` notification.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpponentGone {
    /// Whether the opponent is currently gone.
    #[serde(default)]
    pub gone: bool,
    /// Seconds until the win can be claimed, present once the opponent has been
    /// gone long enough for a claim to become available.
    #[serde(default)]
    pub claim_win_in_seconds: Option<u32>,
}

/// Parse a single game-stream NDJSON line into a [`GameEvent`].
///
/// Blank lines are keepalives and carry no event, so they parse to `None`. A
/// non-blank line that is not valid event JSON is an error.
pub fn parse_game_line(line: &str) -> Result<Option<GameEvent>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(line)
        .map(Some)
        .map_err(|e| Error::Decode(format!("game stream line: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_keepalive_line_yields_no_event() {
        assert_eq!(parse_game_line("").unwrap(), None);
        assert_eq!(parse_game_line("   ").unwrap(), None);
    }

    #[test]
    fn game_full_parses_players_and_initial_state() {
        let line = r#"{"type":"gameFull","id":"abcd1234","white":{"id":"seaborg","name":"seaborg"},"black":{"id":"alice","name":"Alice"},"initialFen":"startpos","state":{"type":"gameState","moves":"e2e4","wtime":300000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}}"#;
        match parse_game_line(line).unwrap().unwrap() {
            GameEvent::GameFull(full) => {
                assert_eq!(full.id, "abcd1234");
                assert_eq!(full.white.id.as_deref(), Some("seaborg"));
                assert_eq!(full.black.id.as_deref(), Some("alice"));
                assert_eq!(full.initial_fen.as_deref(), Some("startpos"));
                assert_eq!(full.state.moves, "e2e4");
                assert_eq!(full.state.wtime, 300000);
                assert!(full.state.is_ongoing());
            }
            other => panic!("expected gameFull, got {other:?}"),
        }
    }

    #[test]
    fn ai_opponent_has_no_id() {
        // An AI side carries only its level, so its `id` is absent and cannot be
        // mistaken for the bot's own account id.
        let line = r#"{"type":"gameFull","id":"g","white":{"id":"seaborg","name":"seaborg"},"black":{"aiLevel":5},"state":{"type":"gameState","moves":"","wtime":60000,"btime":60000,"winc":0,"binc":0,"status":"started"}}"#;
        match parse_game_line(line).unwrap().unwrap() {
            GameEvent::GameFull(full) => {
                assert_eq!(full.white.id.as_deref(), Some("seaborg"));
                assert_eq!(full.black.id, None);
            }
            other => panic!("expected gameFull, got {other:?}"),
        }
    }

    #[test]
    fn game_state_parses_clocks_and_status() {
        let line = r#"{"type":"gameState","moves":"e2e4 e7e5","wtime":298000,"btime":300000,"winc":3000,"binc":3000,"status":"started"}"#;
        match parse_game_line(line).unwrap().unwrap() {
            GameEvent::GameState(state) => {
                assert_eq!(state.moves, "e2e4 e7e5");
                assert_eq!(state.wtime, 298000);
                assert_eq!(state.btime, 300000);
                assert_eq!(state.winc, 3000);
                assert_eq!(state.binc, 3000);
                assert!(state.is_ongoing());
            }
            other => panic!("expected gameState, got {other:?}"),
        }
    }

    #[test]
    fn terminal_status_is_not_ongoing() {
        for status in [
            "mate",
            "resign",
            "outoftime",
            "draw",
            "aborted",
            "stalemate",
        ] {
            let line = format!(
                r#"{{"type":"gameState","moves":"e2e4","wtime":1,"btime":1,"winc":0,"binc":0,"status":"{status}"}}"#
            );
            match parse_game_line(&line).unwrap().unwrap() {
                GameEvent::GameState(state) => {
                    assert!(!state.is_ongoing(), "{status} should be terminal");
                }
                other => panic!("expected gameState, got {other:?}"),
            }
        }
    }

    #[test]
    fn chat_and_opponent_gone_parse() {
        match parse_game_line(
            r#"{"type":"chatLine","username":"alice","text":"hi","room":"player"}"#,
        )
        .unwrap()
        .unwrap()
        {
            GameEvent::ChatLine(chat) => {
                assert_eq!(chat.username, "alice");
                assert_eq!(chat.text, "hi");
                assert_eq!(chat.room, "player");
            }
            other => panic!("expected chatLine, got {other:?}"),
        }

        match parse_game_line(r#"{"type":"opponentGone","gone":true,"claimWinInSeconds":30}"#)
            .unwrap()
            .unwrap()
        {
            GameEvent::OpponentGone(gone) => {
                assert!(gone.gone);
                assert_eq!(gone.claim_win_in_seconds, Some(30));
            }
            other => panic!("expected opponentGone, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_type_maps_to_other() {
        assert_eq!(
            parse_game_line(r#"{"type":"gameFinish","game":{"id":"x"}}"#)
                .unwrap()
                .unwrap(),
            GameEvent::Other
        );
    }

    #[test]
    fn malformed_line_is_an_error() {
        assert!(parse_game_line("{not json").is_err());
    }
}
