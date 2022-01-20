//! Methods for reading UCI commands sent from a GUI from stdin, parsing
//! the commands, and transmitting responses to stdout.

mod inbound;
mod outbound;
mod parse;

pub use outbound::Res;
pub use parse::{Req, Pos};

/// Dummy struct collecting UCI functionality.
pub struct Uci {}
