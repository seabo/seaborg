//! An Engine session.
//!
//! This structure is created when the user loads the program
//! with the `--uci` flag, or when the `uci` command is sent.
//! It runs in the main program thread, and launches a new
//! thread for the search. The two threads communicate by
//! message passing, with the `crossbeam_channel` crate.
//!
//! `UciSess` receives incoming commands from the GUI on stdin,
//! parses these commands, and issues internal engine commands
//! via the communication channel.

// uci::sess - manages a uci_session, which includes:
//     DONE    - holding a handle to the command line buffer
//     DONE    - running an infinite loop waiting for command line input
//             - holding `crossbeam_channel` tx and rx handles
//             - spawning the main search thread and holding a handle to it
//     DONE    - communicating the latest commands from the GUI through to the engine
//             - and passing search info back from the engine to the GUI

pub mod cmd;
pub mod out;

use std::io::{self, Stdin};
use std::thread;

// TODO: this needs to move to the engine crate and be imported to here
#[derive(Debug)]
pub enum EngineCommand {
    SetPosition(String),
    SetStartpos,
    SetOption,
    Go,
}

#[derive(Debug)]
pub enum GuiCommand {
    ReadyOk,
}

/// A `SessionCommand` is a command which queries the UCI session for readiness
/// - this can either be the `uci` command which normally is only sent at engine
/// startup by the GUI to request UCI mode, or `isready` which is sent by the GUI
/// after a variety of commands which may take time to complete and therefore
/// require the engine and GUI to be synchronized.  
#[derive(Debug)]
pub enum SessionCommand {
    /// The initial `uci` command was sent by the GUI, requesting the engine
    /// to confirm its is ready to participate in a UCI session.
    Uci,

    /// An `isready` command was sent by the GUI in order to synchronize with the
    /// engine. The GUI expects to receive back a `readyok` response immediately
    /// if everything is fine (including when a search is running, in which case)
    /// the search should not be interrupted.
    IsReady,

    /// An engine command was sent, which the UCI session should process and pass
    /// through to the engine process.
    Engine(EngineCommand),
}
