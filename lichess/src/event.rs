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
    /// A challenge the bot sent was declined by its recipient. Carried so
    /// matchmaking can avoid immediately re-challenging a bot that just declined.
    ChallengeDeclined {
        /// The declined challenge, including who declined it.
        challenge: DeclinedChallenge,
    },
    /// An incoming challenge was withdrawn by the challenger before it became a
    /// game. Carried so the acceptance path can free a slot it had reserved for
    /// that challenge; without it the reservation would linger until it expired.
    ChallengeCanceled {
        /// The canceled challenge, identified by id.
        challenge: CanceledChallenge,
    },
    /// Any other event type (future additions). Kept so the stream tolerates
    /// events the bot does not handle.
    #[serde(other)]
    Other,
}

/// The subset of a declined challenge the bot acts on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclinedChallenge {
    /// The account the challenge was sent to — the one that declined it. Absent
    /// for open (untargeted) challenges, which matchmaking never sends.
    #[serde(default)]
    pub dest_user: Option<UserRef>,
}

/// The subset of a canceled challenge the bot acts on: just its id, which
/// matches the id of any slot the acceptance path reserved for it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CanceledChallenge {
    /// The challenge id, used to release a reserved slot.
    pub id: String,
}

/// A minimal reference to an account by id.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UserRef {
    /// The account id.
    pub id: String,
}

/// Reference to a game carried by `gameStart` / `gameFinish` events.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRef {
    /// The game's Lichess id, used to build game-stream and move URLs.
    pub id: String,
}

/// A challenge event carried on the account event stream.
///
/// The stream delivers this for challenges in both directions: ones sent *to*
/// the bot and echoes of ones the bot itself *issued*. [`Challenge::is_from_self`]
/// distinguishes them so the bot never tries to accept its own outgoing
/// challenge (which Lichess answers with a 404).
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
    /// Which way the challenge points relative to the authenticated account, when
    /// Lichess reports it. `Out` marks a challenge the bot issued. Optional per the
    /// API spec, so it only ever corroborates the challenger-identity check.
    #[serde(default)]
    pub direction: Option<Direction>,
}

/// The direction of a challenge relative to the authenticated account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// An incoming challenge, sent to the bot.
    In,
    /// An outgoing challenge, issued by the bot.
    Out,
}

impl Challenge {
    /// Whether this challenge was issued by the bot itself.
    ///
    /// The account event stream echoes the bot's own outgoing challenges
    /// alongside genuine incoming ones. Accepting an outgoing challenge is a
    /// nonsensical request Lichess rejects with a 404, so these must be ignored.
    ///
    /// The challenger's account id is the authoritative signal: Lichess sets it
    /// to the bot's own id on an echoed outgoing challenge. The `direction` field
    /// is optional, so it cannot be relied on alone; when Lichess does include it,
    /// an `Out` direction confirms the same conclusion.
    pub fn is_from_self(&self, own_id: &str) -> bool {
        self.challenger.id == own_id || self.direction == Some(Direction::Out)
    }
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
    fn challenge_declined_carries_the_declining_bot() {
        let line = r#"{"type":"challengeDeclined","challenge":{"id":"x","status":"declined","destUser":{"id":"fussybot","name":"FussyBot"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::ChallengeDeclined { challenge } => {
                assert_eq!(
                    challenge.dest_user.map(|u| u.id),
                    Some("fussybot".to_string())
                );
            }
            other => panic!("expected challengeDeclined, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_type_maps_to_other() {
        // A type the bot does not model must not fail the stream.
        assert_eq!(
            parse_line(r#"{"type":"someFutureEvent","payload":{"id":"x"}}"#)
                .unwrap()
                .unwrap(),
            Event::Other
        );
    }

    #[test]
    fn challenge_canceled_carries_the_challenge_id() {
        // Real challengeCanceled JSON carries a full challenge object; only the id
        // is modeled, and the surrounding fields must be tolerated.
        let line = r#"{"type":"challengeCanceled","challenge":{"id":"abc123","status":"canceled","challenger":{"id":"alice","name":"alice"},"destUser":{"id":"seaborg","name":"seaborg"},"variant":{"key":"standard"},"rated":false,"timeControl":{"type":"clock","limit":300,"increment":3}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::ChallengeCanceled { challenge } => assert_eq!(challenge.id, "abc123"),
            other => panic!("expected challengeCanceled, got {other:?}"),
        }
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

    #[test]
    fn direction_out_marks_an_outgoing_challenge() {
        // Real Lichess challenge JSON for the bot's own outgoing matchmaking
        // challenge, carrying fields the bot does not parse (speed, perf, color,
        // finalColor, destUser) to confirm they are tolerated.
        let line = r#"{"type":"challenge","challenge":{"id":"out01","direction":"out","status":"created","challenger":{"id":"seaborg","name":"seaborg","title":"BOT","rating":1800},"destUser":{"id":"maia1","name":"maia1","title":"BOT","rating":1700},"variant":{"key":"standard"},"rated":false,"speed":"blitz","timeControl":{"type":"clock","limit":300,"increment":3,"show":"5+3"},"color":"random","finalColor":"white","perf":{"icon":"","name":"Blitz"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::Challenge { challenge } => {
                assert_eq!(challenge.direction, Some(Direction::Out));
                // The challenger id matches; direction merely corroborates.
                assert!(challenge.is_from_self("seaborg"));
            }
            other => panic!("expected challenge, got {other:?}"),
        }
    }

    #[test]
    fn is_from_self_uses_challenger_identity_when_direction_absent() {
        // No direction field: identity alone must still flag the bot's own
        // challenge, and must not flag a stranger's.
        let line = r#"{"type":"challenge","challenge":{"id":"c","rated":false,"variant":{"key":"standard"},"timeControl":{"type":"clock","limit":300,"increment":3},"challenger":{"id":"seaborg","name":"seaborg","title":"BOT"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::Challenge { challenge } => {
                assert_eq!(challenge.direction, None);
                assert!(challenge.is_from_self("seaborg"));
                assert!(!challenge.is_from_self("someone-else"));
            }
            other => panic!("expected challenge, got {other:?}"),
        }
    }

    #[test]
    fn incoming_direction_is_not_from_self() {
        let line = r#"{"type":"challenge","challenge":{"id":"in01","direction":"in","rated":false,"variant":{"key":"standard"},"timeControl":{"type":"clock","limit":300,"increment":3},"challenger":{"id":"alice","name":"alice"}}}"#;
        match parse_line(line).unwrap().unwrap() {
            Event::Challenge { challenge } => {
                assert_eq!(challenge.direction, Some(Direction::In));
                assert!(!challenge.is_from_self("seaborg"));
            }
            other => panic!("expected challenge, got {other:?}"),
        }
    }
}
