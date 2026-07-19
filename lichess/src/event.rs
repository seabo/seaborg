//! Account event-stream types and NDJSON parsing.
//!
//! The event stream (`GET /api/stream/event`) emits one JSON object per line.
//! These types model only the fields the bot acts on; unknown event types and
//! unknown object fields are ignored so a Lichess API addition does not break
//! the stream.

use serde::Deserialize;

use crate::error::{Error, Result};

/// One event from the account event stream.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    /// An incoming challenge awaiting an accept/decline decision.
    Challenge {
        /// The challenge details.
        challenge: Challenge,
    },
    /// A game the bot is now in has started.
    GameStart {
        /// The started game.
        game: GameRef,
    },
    /// A game the bot was in has ended.
    GameFinish {
        /// The finished game.
        game: GameRef,
    },
    /// Any other event type (challenge canceled, challenge declined, and future
    /// additions). Kept so the stream tolerates events the bot does not handle.
    #[serde(other)]
    Other,
}

/// Reference to a game carried by `gameStart` / `gameFinish` events.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRef {
    /// The game's Lichess id, used to build game-stream and move URLs.
    pub id: String,
}

/// An incoming challenge.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    /// The challenge id, used to accept or decline.
    pub id: String,
    /// Whether the challenge is rated (as opposed to casual).
    pub rated: bool,
    /// The game variant.
    pub variant: Variant,
    /// The time control.
    pub time_control: TimeControl,
    /// The account issuing the challenge.
    pub challenger: Challenger,
}

/// A game variant, identified by its stable key.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    /// The variant key, for example `standard` or `chess960`.
    pub key: String,
}

/// A challenge's time control.
#[derive(Debug, Clone, PartialEq, Deserialize)]
// `rename_all` renames the variant tags (`clock`, `correspondence`, ...);
// `rename_all_fields` is needed as well to map fields like `daysPerTurn`.
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TimeControl {
    /// A real-time clock with an initial time and per-move increment.
    Clock {
        /// Initial time in seconds.
        limit: u32,
        /// Increment per move in seconds.
        increment: u32,
    },
    /// A correspondence game with a per-move day budget.
    Correspondence {
        /// Days allowed per move, if specified.
        #[serde(default)]
        days_per_turn: Option<u32>,
    },
    /// No time limit.
    Unlimited,
}

/// The account issuing a challenge.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenger {
    /// The challenger's account id.
    pub id: String,
    /// The challenger's display name.
    pub name: String,
    /// The challenger's rating in the relevant pool, if published.
    #[serde(default)]
    pub rating: Option<u32>,
    /// The challenger's title, if any. `BOT` marks another bot account.
    #[serde(default)]
    pub title: Option<String>,
}

impl Challenger {
    /// Whether the challenger is another bot account.
    pub fn is_bot(&self) -> bool {
        self.title.as_deref() == Some("BOT")
    }
}

/// Parse a single NDJSON stream line into an [`Event`].
///
/// Blank lines are keepalives and carry no event, so they parse to `None`. A
/// non-blank line that is not valid event JSON is an error.
pub fn parse_line(line: &str) -> Result<Option<Event>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(line)
        .map(Some)
        .map_err(|e| Error::Decode(format!("event line: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_keepalive_line_yields_no_event() {
        assert_eq!(parse_line("").unwrap(), None);
        assert_eq!(parse_line("   ").unwrap(), None);
    }

    #[test]
    fn challenge_line_parses_fields_used_by_policy() {
        let line = r#"{"type":"challenge","challenge":{"id":"abc","rated":true,"variant":{"key":"standard"},"timeControl":{"type":"clock","limit":180,"increment":2},"challenger":{"id":"bo","name":"Bo","rating":2100,"title":"BOT"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::Challenge { challenge } => {
                assert_eq!(challenge.id, "abc");
                assert!(challenge.rated);
                assert_eq!(challenge.variant.key, "standard");
                assert_eq!(
                    challenge.time_control,
                    TimeControl::Clock {
                        limit: 180,
                        increment: 2
                    }
                );
                assert_eq!(challenge.challenger.rating, Some(2100));
                assert!(challenge.challenger.is_bot());
            }
            other => panic!("expected challenge, got {other:?}"),
        }
    }

    #[test]
    fn game_events_parse() {
        assert_eq!(
            parse_line(r#"{"type":"gameStart","game":{"id":"g1"}}"#)
                .unwrap()
                .unwrap(),
            Event::GameStart {
                game: GameRef {
                    id: "g1".to_string()
                }
            }
        );
        assert_eq!(
            parse_line(r#"{"type":"gameFinish","game":{"id":"g1"}}"#)
                .unwrap()
                .unwrap(),
            Event::GameFinish {
                game: GameRef {
                    id: "g1".to_string()
                }
            }
        );
    }

    #[test]
    fn unknown_event_type_maps_to_other() {
        assert_eq!(
            parse_line(r#"{"type":"challengeCanceled","challenge":{"id":"x"}}"#)
                .unwrap()
                .unwrap(),
            Event::Other
        );
    }

    #[test]
    fn correspondence_and_unlimited_time_controls_parse() {
        let line = r#"{"type":"challenge","challenge":{"id":"c","rated":false,"variant":{"key":"standard"},"timeControl":{"type":"correspondence","daysPerTurn":3},"challenger":{"id":"a","name":"A"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::Challenge { challenge } => assert_eq!(
                challenge.time_control,
                TimeControl::Correspondence {
                    days_per_turn: Some(3)
                }
            ),
            other => panic!("expected challenge, got {other:?}"),
        }
    }

    #[test]
    fn missing_challenger_rating_is_none() {
        let line = r#"{"type":"challenge","challenge":{"id":"c","rated":false,"variant":{"key":"standard"},"timeControl":{"type":"unlimited"},"challenger":{"id":"a","name":"A"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::Challenge { challenge } => {
                assert_eq!(challenge.challenger.rating, None);
                assert!(!challenge.challenger.is_bot());
                assert_eq!(challenge.time_control, TimeControl::Unlimited);
            }
            other => panic!("expected challenge, got {other:?}"),
        }
    }

    #[test]
    fn malformed_line_is_an_error() {
        assert!(parse_line("{not json").is_err());
    }
}
