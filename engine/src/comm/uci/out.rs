//! Emit engine uci commands to stdout.

// TODO: get these details from  root package Cargo.toml somehow
const NAME: &str = "rchess";
const VERSION: &str = "0.1.0";
const AUTHORS: &str = "George Seabridge <georgeseabridge@gmail.com>";

pub struct UciOut {}

impl UciOut {
    pub fn identify() {
        println!("id name {} {}", NAME, VERSION);
        println!("id author {}", AUTHORS);
    }

    pub fn options() {
        // TODO: currently no options available to set the engine
    }

    pub fn uciok() {
        println!("uciok");
    }

    pub fn readyok() {
        println!("readyok");
    }

    pub fn new_line() {
        println!();
    }
}
