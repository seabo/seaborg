//! Error type shared across the crate.

use std::fmt;
use std::time::Duration;

/// Result alias for fallible operations in this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while configuring or running the bot.
///
/// The variants map onto the distinct failures a user needs to act on
/// differently: a missing token is a setup problem, a rejected token is an
/// authentication problem, and a non-bot account needs the upgrade command.
#[derive(Debug)]
pub enum Error {
    /// The token environment variable is unset or empty.
    MissingToken,
    /// The API rejected the token (HTTP 401). The token is present but invalid
    /// or lacks the required scopes.
    Unauthorized,
    /// The account behind the token is not a BOT account, so it cannot use the
    /// Bot API. Resolve with `seaborg lichess upgrade`.
    NotBotAccount {
        /// The account username, for a message the user can act on.
        username: String,
    },
    /// The account cannot be upgraded to a BOT account because it has already
    /// played games. Lichess only upgrades accounts with zero games.
    UpgradeIneligible {
        /// The account username.
        username: String,
        /// The number of games already played.
        games: u64,
    },
    /// The configuration file exists but could not be read or parsed.
    Config(String),
    /// An HTTP request failed at the transport level (connection, TLS, or a
    /// non-success status other than 401 or 429).
    Http(String),
    /// The API returned HTTP 429 (too many requests). `retry_after` carries the
    /// server's `Retry-After` hint in seconds when it sent one.
    RateLimited {
        /// How long the server asked the client to wait, if it said.
        retry_after: Option<Duration>,
    },
    /// A response body was not the JSON this crate expected.
    Decode(String),
}

impl Error {
    /// Whether this error is a transient network condition worth retrying by
    /// reconnecting or backing off, as opposed to a terminal fault.
    ///
    /// A dropped connection or a rate-limit response is recoverable; a rejected
    /// token or a decode failure (a protocol change or a bug) is not, and must
    /// surface rather than spin in a reconnect loop.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Error::Http(_) | Error::RateLimited { .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MissingToken => write!(
                f,
                "no Lichess token: set the {} environment variable to a personal API token \
                 with the bot:play scope",
                crate::TOKEN_ENV_VAR
            ),
            Error::Unauthorized => write!(
                f,
                "Lichess rejected the token: check that {} holds a valid token with the \
                 bot:play scope",
                crate::TOKEN_ENV_VAR
            ),
            Error::NotBotAccount { username } => write!(
                f,
                "account '{username}' is not a BOT account; run `seaborg lichess upgrade` to \
                 convert it (irreversible, requires an account with zero games)"
            ),
            Error::UpgradeIneligible { username, games } => write!(
                f,
                "account '{username}' has played {games} game(s); Lichess only upgrades \
                 accounts that have never played a game"
            ),
            Error::Config(detail) => write!(f, "configuration error: {detail}"),
            Error::Http(detail) => write!(f, "HTTP request failed: {detail}"),
            Error::RateLimited { retry_after } => match retry_after {
                Some(wait) => write!(
                    f,
                    "rate limited by Lichess (HTTP 429); retry after {}s",
                    wait.as_secs()
                ),
                None => write!(f, "rate limited by Lichess (HTTP 429)"),
            },
            Error::Decode(detail) => write!(f, "could not decode Lichess response: {detail}"),
        }
    }
}

impl std::error::Error for Error {}
