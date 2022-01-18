//! Emit engine uci commands to stdout.

use std::io::{self, Stdout};

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

pub struct UciOut {}

impl UciOut {
    pub fn identify(h: &Stdout) {
        // TODO: get these details from Cargo.toml somehow
        println!("id name {}", NAME);
        println!("id author {}", AUTHORS);
    }
}
