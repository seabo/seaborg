//! Lichess bot client.
//!
//! This crate connects Seaborg to the Lichess Bot API: it authenticates with a
//! personal API token, loads a challenge-acceptance policy from TOML, opens the
//! account event stream, and accepts or declines incoming challenges. Each
//! accepted game is played to completion by a per-game worker (see [`game`]),
//! which streams the game and replies with the engine's moves.
//!
//! The HTTP surface sits behind the [`transport::Transport`] trait so the event
//! loop and game loop can both be exercised against recorded NDJSON without
//! touching the network.

pub mod account;
pub mod backoff;
pub mod client;
pub mod config;
pub mod error;
pub mod event;
pub mod game;
pub mod game_stream;
pub mod matchmaking;
pub mod policy;
pub mod run;
pub mod shutdown;
pub mod transport;

pub use error::{Error, Result};

/// Base URL of the Lichess HTTP API. Every request path in this crate is
/// relative to this origin.
pub const DEFAULT_BASE_URL: &str = "https://lichess.org";

/// Environment variable holding the bot's personal API access token.
pub const TOKEN_ENV_VAR: &str = "LICHESS_BOT_TOKEN";
