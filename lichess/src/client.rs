//! Typed Lichess API calls over a [`Transport`].
//!
//! [`LichessClient`] turns the raw string bodies of [`Transport`] into the
//! crate's domain types and knows the Lichess endpoint paths. It is generic over
//! the transport so the same logic runs against the live API or a test double.

use crate::account::Account;
use crate::error::{Error, Result};
use crate::event::{parse_line, Event};
use crate::game_stream::{parse_game_line, GameEvent};
use crate::matchmaking::{parse_bot_line, BotInfo, ChallengeSpec};
use crate::policy::DeclineReason;
use crate::transport::Transport;

/// A Lichess API client bound to one authenticated account.
pub struct LichessClient<T: Transport> {
    transport: T,
}

impl<T: Transport> LichessClient<T> {
    /// Wrap `transport` in a typed client.
    pub fn new(transport: T) -> LichessClient<T> {
        LichessClient { transport }
    }

    /// Borrow the underlying transport, for tests that assert on a recording
    /// double after driving the client.
    #[cfg(test)]
    pub(crate) fn transport(&self) -> &T {
        &self.transport
    }

    /// Fetch the authenticated account. Doubles as a token check: an invalid
    /// token makes this fail with [`Error::Unauthorized`].
    pub fn account(&self) -> Result<Account> {
        let body = self.transport.get("/api/account")?;
        serde_json::from_str(&body).map_err(|e| Error::Decode(format!("account: {e}")))
    }

    /// Accept the challenge with the given id.
    pub fn accept_challenge(&self, id: &str) -> Result<()> {
        self.transport
            .post_empty(&format!("/api/challenge/{id}/accept"))
            .map(drop)
    }

    /// Decline the challenge with the given id, reporting `reason`.
    pub fn decline_challenge(&self, id: &str, reason: DeclineReason) -> Result<()> {
        self.transport
            .post_form(
                &format!("/api/challenge/{id}/decline"),
                &[("reason", reason.as_str())],
            )
            .map(drop)
    }

    /// Fetch up to `count` currently-online bots for matchmaking.
    ///
    /// `GET /api/bot/online` is an NDJSON stream of one bot per line; this reads it
    /// to completion (bounded by `count`) into typed [`BotInfo`]s. Blank keepalive
    /// lines carry no bot and are skipped.
    pub fn online_bots(&self, count: u32) -> Result<Vec<BotInfo>> {
        let body = self.transport.get(&format!("/api/bot/online?nb={count}"))?;
        body.lines()
            .filter_map(|line| parse_bot_line(line).transpose())
            .collect()
    }

    /// Issue an outgoing challenge to `username` for the game described by `spec`.
    ///
    /// Sends the challenge with a random color so the bot does not always take the
    /// same side; the clock and rated flag come from the composed spec.
    pub fn create_challenge(&self, username: &str, spec: &ChallengeSpec) -> Result<()> {
        let limit = spec.initial_seconds.to_string();
        let increment = spec.increment_seconds.to_string();
        let rated = spec.rated.to_string();
        self.transport
            .post_form(
                &format!("/api/challenge/{username}"),
                &[
                    ("rated", rated.as_str()),
                    ("clock.limit", limit.as_str()),
                    ("clock.increment", increment.as_str()),
                    ("variant", spec.variant.as_str()),
                    ("color", "random"),
                ],
            )
            .map(drop)
    }

    /// Upgrade the authenticated account to a BOT account.
    ///
    /// This is irreversible and only succeeds for an account that has never
    /// played a game and whose token carries the `bot:play` scope.
    pub fn upgrade_to_bot(&self) -> Result<()> {
        self.transport
            .post_empty("/api/bot/account/upgrade")
            .map(drop)
    }

    /// Open the account event stream, yielding one item per JSON line.
    ///
    /// Each item is `Ok(Some(event))` for a real event, `Ok(None)` for a blank
    /// keepalive line, or `Err` for a transport or parse failure. Keepalives are
    /// surfaced rather than dropped so the consumer regains control frequently
    /// enough to notice a shutdown request between real events.
    pub fn event_stream(&self) -> Result<impl Iterator<Item = Result<Option<Event>>>> {
        let lines = self.transport.open_stream("/api/stream/event")?;
        Ok(lines.map(|line| line.and_then(|line| parse_line(&line))))
    }

    /// Open a game's stream, yielding one item per JSON line.
    ///
    /// Like [`event_stream`](Self::event_stream), each item is
    /// `Ok(Some(event))`, `Ok(None)` for a keepalive, or `Err`; keepalives are
    /// surfaced so a game worker can observe a shutdown request promptly even
    /// while it is the opponent's turn.
    pub fn game_stream(
        &self,
        game_id: &str,
    ) -> Result<impl Iterator<Item = Result<Option<GameEvent>>>> {
        let lines = self
            .transport
            .open_stream(&format!("/api/bot/game/stream/{game_id}"))?;
        Ok(lines.map(|line| line.and_then(|line| parse_game_line(&line))))
    }

    /// Play `uci` in the given game via the bot move endpoint.
    pub fn play_move(&self, game_id: &str, uci: &str) -> Result<()> {
        self.transport
            .post_empty(&format!("/api/bot/game/{game_id}/move/{uci}"))
            .map(drop)
    }

    /// Resign the given game via the bot resign endpoint.
    ///
    /// Used on shutdown to end an in-flight game cleanly instead of dropping the
    /// connection mid-game and leaving the bot to flag on time.
    pub fn resign_game(&self, game_id: &str) -> Result<()> {
        self.transport
            .post_empty(&format!("/api/bot/game/{game_id}/resign"))
            .map(drop)
    }
}
