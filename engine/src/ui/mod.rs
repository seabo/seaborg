//! The local browser UI: a loopback-only HTTP server over the authoritative game controller.
//!
//! The server owns no chess rules. It publishes versioned snapshots produced by
//! [`crate::game::GameController`], accepts a fixed set of bounded commands, and streams updates
//! over Server-Sent Events. Everything it serves is embedded in the executable, and it binds only
//! to 127.0.0.1.

mod http;
mod json;
mod server;
mod session;
mod wire;

#[cfg(test)]
mod tests;

pub use server::{
    bind, open_browser, UiConfig, UiError, UiHandle, UiServer, MAX_CONNECTIONS, MAX_REQUEST_BODY,
};
