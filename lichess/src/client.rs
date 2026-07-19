//! Typed Lichess API calls over a [`Transport`].
//!
//! [`LichessClient`] turns the raw string bodies of [`Transport`] into the
//! crate's domain types and knows the Lichess endpoint paths. It is generic over
//! the transport so the same logic runs against the live API or a test double.

use crate::account::Account;
use crate::error::{Error, Result};
use crate::event::{parse_line, Event};
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

    /// Upgrade the authenticated account to a BOT account.
    ///
    /// This is irreversible and only succeeds for an account that has never
    /// played a game and whose token carries the `bot:play` scope.
    pub fn upgrade_to_bot(&self) -> Result<()> {
        self.transport
            .post_empty("/api/bot/account/upgrade")
            .map(drop)
    }

    /// Open the account event stream, yielding one [`Event`] per JSON line.
    ///
    /// Keepalive blank lines are dropped; a transport error or an unparseable
    /// line surfaces as an `Err` item without ending the stream's type.
    pub fn event_stream(&self) -> Result<impl Iterator<Item = Result<Event>>> {
        let lines = self.transport.open_stream("/api/stream/event")?;
        Ok(lines.filter_map(|line| match line {
            Err(e) => Some(Err(e)),
            Ok(line) => match parse_line(&line) {
                Ok(None) => None,
                Ok(Some(event)) => Some(Ok(event)),
                Err(e) => Some(Err(e)),
            },
        }))
    }
}
