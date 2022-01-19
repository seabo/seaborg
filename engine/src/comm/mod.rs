//! Orchestrates an engine session.
//!
//! This module owns the `Search` structure which manages its own thread for
//! running the search in. The `Session` then spawns a further two threads,
//! one (the GuiIn thread) to watch for input from the GUI, and one (the
//! EngineReport thread) to watch for responses from the engine thread.
//!
//! The `Session` awaits these messages by running an infinite loop and holding
//! a single `crossbeam_channel` receiver which recieves combined input from
//! both the GuiIn thread and the EngineReport thread. The `Session` then
//! handles these messages. The actions it takes when receiving a message from
//! each source are:
//! - GuiIn: updating the search parameters in the owned `Search` struct and
//!   calling methods on it to launch the search
//! - EngineReport: deciding what to communicate back to the GUI and calling
//!   methods from the UCI module to issue the relevant text to stdout.
pub mod uci;

use core::init::init_globals;
use uci::cmd::UciParser;
use uci::out::UciOut;

use crossbeam_channel::unbounded;
use std::io::Stdin;

pub struct Session {
    stdin: Stdin,
}

impl Session {
    pub fn new() -> Self {
        Session { stdin: io::stdin() }
    }

    pub fn run(&mut self) {
        loop {
            // Read a command from the GUI
            match UciParser::next_command(&self.stdin) {
                Ok(cmd) => self.execute_command(cmd),
                Err(err) => println!("Parsing error: {:?}", err),
            }
        }
    }

    fn execute_command(&mut self, cmd: SessionCommand) {
        match cmd {
            SessionCommand::Uci => self.return_uci_handshake(),
            SessionCommand::IsReady => self.initialize_engine(),
            _ => todo!(),
        }
    }

    fn return_uci_handshake(&mut self) {
        UciOut::identify();
        UciOut::options();
        UciOut::new_line();
        UciOut::uciok();
    }

    fn unexpected_command(&mut self) {
        // When we received an unexpected command, we simply ignore it.
    }

    fn initialize_engine(&mut self) {
        // Following the example of Stockfish, we don't bother keeping any state to
        // enforce that a uci handshake has actually taken place before this point.
        // If not, we just plow on regardless.
        let (s, r) = unbounded();

        let engine_thread = thread::spawn(move || {
            init_globals();
            s.send(GuiCommand::ReadyOk).unwrap();
        });

        match r.recv().unwrap() {
            GuiCommand::ReadyOk => UciOut::readyok(),
        }
    }
}
