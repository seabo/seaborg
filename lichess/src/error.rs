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
    /// non-success status other than 401, 404, or 429).
    Http(String),
    /// The API returned HTTP 404 (the addressed resource does not exist). For a
    /// challenge accept this is the spec's "challenge gone" outcome — the
    /// challenger canceled it or it expired before the accept landed — and is
    /// benign rather than a fault.
    NotFound,
    /// The API returned HTTP 429 (too many requests), so *this* account is over
    /// one of Lichess's limits. Which limit fired matters to the caller: a limit
    /// on the account says nothing about the opponent a request happened to name,
    /// so it must not be mistaken for a refusal by that opponent.
    RateLimited {
        /// Lichess's identifier for the limit that fired, when the response body
        /// named one (for example `bot.vsBot.day`, the cap on how many games a
        /// bot may play against other bots in a day).
        key: Option<String>,
        /// How long the server asked the client to wait, if it said. Lichess
        /// reports this either in the response body or, for some endpoints, in a
        /// `Retry-After` header.
        retry_after: Option<Duration>,
    },
    /// The API refused a request because the *other* account it names is over a
    /// Lichess limit, not because this account did anything wrong.
    ///
    /// Lichess reports this as an HTTP 400 whose body carries the same rate-limit
    /// envelope as a 429. It arises when challenging a bot that has already played
    /// its daily allowance of games against other bots: challenging anyone else is
    /// still fine, so only the named account should be skipped.
    OpponentRateLimited {
        /// Lichess's identifier for the limit the opponent is over.
        key: String,
        /// How long until the opponent's limit clears, if the body said.
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
    ///
    /// A 404 counts as recoverable so a stray one on any call is swallowed like a
    /// transient fault rather than ending the bot; the accept path handles it
    /// explicitly as the expected "challenge gone" outcome before this general
    /// tolerance applies.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::Http(_)
                | Error::RateLimited { .. }
                | Error::OpponentRateLimited { .. }
                | Error::NotFound
        )
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
            Error::NotFound => write!(f, "resource not found (HTTP 404)"),
            Error::RateLimited { key, retry_after } => {
                write!(f, "rate limited by Lichess (HTTP 429)")?;
                if let Some(key) = key {
                    write!(f, " [{key}]")?;
                }
                match retry_after {
                    Some(wait) => write!(f, "; retry after {}s", wait.as_secs()),
                    None => Ok(()),
                }
            }
            Error::OpponentRateLimited { key, retry_after } => {
                write!(f, "opponent is rate limited by Lichess [{key}]")?;
                match retry_after {
                    Some(wait) => write!(f, "; clears in {}s", wait.as_secs()),
                    None => Ok(()),
                }
            }
            Error::Decode(detail) => write!(f, "could not decode Lichess response: {detail}"),
        }
    }
}

impl std::error::Error for Error {}
