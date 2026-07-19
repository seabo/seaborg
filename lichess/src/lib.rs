//! Lichess bot client.
//!
//! This crate connects Seaborg to the Lichess Bot API: it authenticates with a
//! personal API token, loads a challenge-acceptance policy from TOML, opens the
//! account event stream, and accepts or declines incoming challenges. Actually
//! playing the moves of an accepted game is handled by a later stage and is not
//! wired up here (see [`game`] for the handoff point).
//!
//! The HTTP surface sits behind the [`transport::Transport`] trait so the event
//! loop can be exercised against recorded NDJSON without touching the network.

pub mod account;
pub mod client;
pub mod config;
pub mod error;
pub mod event;
pub mod game;
pub mod policy;
pub mod run;
pub mod transport;

pub use error::{Error, Result};

/// Base URL of the Lichess HTTP API. Every request path in this crate is
/// relative to this origin.
pub const DEFAULT_BASE_URL: &str = "https://lichess.org";

/// Environment variable holding the bot's personal API access token.
pub const TOKEN_ENV_VAR: &str = "LICHESS_BOT_TOKEN";
