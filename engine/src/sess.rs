//! An engine session.
//!
//! This module owns the `Engine` structure which manages its own thread for
//! running the search.
//!
//! The `Session` holds a `Comm` struct which spawns a further two threads,
//! one to watch for input from the GUI, and one to watch for responses from
//! the engine thread.
//!
//! The `Session` awaits these messages by running an infinite loop and holding
//! a single `crossbeam_channel` receiver which recieves combined input from
//! both the gui input thread and the engine reporting thread. The `Session` then
//! handles these messages. The actions it takes when receiving a message from
//! each source are:
//! - GUI input: updating the search parameters in the owned `Search` struct and
//!   calling methods on it to launch the search
//! - Engine reports: deciding what to communicate back to the GUI and calling
//!   methods from the UCI module to issue the relevant text to stdout.

use crate::comm::Comm;
use crate::engine::{Command, Engine, Report};
use crate::uci::{Pos, Req, Res};

use crossbeam_channel::{unbounded, Receiver};

/// Represents a message received by the session, either from the GUI or from
/// the engine thread.
pub enum Message {
    FromGui(Req),
    FromEngine(Report),
}

pub struct Session {
    /// The communication module used by the session to orchestrate interactions
    /// between the search thread and the GUI.
    comm: Comm,
    /// A `JoinHandle` for the thread where the engine is running.
    engine: Engine,
    /// `Receiver` of communications from the GUI or Engine.
    rx: Receiver<Message>,
    /// Flag which will be set to true when the session should be quit.
    quit: bool,
}

impl Session {
    pub fn new() -> Self {
        let (tx, rx) = unbounded::<Message>();
        let engine = Engine::new(tx.clone());

        Self {
            quit: false,
            comm: Comm::new(tx),
            engine,
            rx,
        }
    }

    pub fn main_loop(&mut self) {
        loop {
            // TODO: should test on every loop cycle that neither of the
            // listener threads has panicked.
            // Check if we are quitting.
            if self.quit {
                break;
            }

            let result = self.rx.recv();

            match result {
                Ok(msg) => self.handle_message(msg),
                Err(err) => eprintln!("{}", err),
            }
        }
    }

    fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::FromGui(s) => self.handle_gui_message(s),
            Message::FromEngine(r) => self.handle_engine_message(r),
        }
    }

    fn handle_gui_message(&mut self, msg: Req) {
        match msg {
            Req::Uci => self.uci(),
            Req::IsReady => self.isready(),
            Req::UciNewGame => self.new_game(),
            Req::SetPosition(pos) => self.set_position(pos),
            Req::Go => todo!(),
            Req::Quit => self.quit_session(),
        }
    }

    fn handle_engine_message(&mut self, report: Report) {
        let res = match report {
            Report::BestMove => Res::Uciok,
            Report::InitializationComplete => Res::Readyok,
        };
        self.comm.send(res);
    }

    fn uci(&self) {
        self.comm.send(Res::Identify);
        // TODO: send available engine options
        self.comm.send(Res::Uciok);
    }

    fn isready(&mut self) {
        // Initialize engine
        self.initialize_engine();
    }

    fn new_game(&mut self) {
        // Currently we don't do anything special when this
        // command is received, but eventually we might have internal
        // engine state which wants to retain some game state between
        // searches. But the whole point of UCI is to not have to
        // hold any game state, so we just rely on the set position
        // and go commands to tell us everything we need to know.
    }

    fn set_position(&mut self, pos: Pos) {
        self.engine.send(Command::SetPosition(pos));
    }

    fn initialize_engine(&mut self) {
        self.engine.send(Command::Initialize);
    }

    fn quit_session(&mut self) {
        // Shut down the comm threads.
        self.comm.send(Res::Quit);
        self.comm.wait_for_shutdown();

        // Shut down the engine thread.
        self.engine.send(Command::Quit);
        self.engine.wait_for_shutdown();

        // Set the session quit flag to true, so that the main loop
        // breaks.
        self.quit = true;
    }
}
