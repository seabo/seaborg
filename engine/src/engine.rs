use crate::search::pv_search::PVSearch;
use crate::sess::Message;
use core::position::Position;

use crossbeam_channel::{unbounded, Sender};

use std::thread::{self, JoinHandle};

#[derive(Debug)]
pub enum Command {
    Initialize,
    SetPosition,
    Search,
    Halt,
    Quit,
}

/// Represents an engine report. This is passed back from the `Engine` to the
/// `Session`, which then forwards it to the `Comm` module via `comm.send()`.
/// The `Comm` module then takes responsibility for handling the report, usually
/// by converting into a uci response written to stdout.
// TODO: this should live in a more appropriate module eventually.
pub enum Report {
    /// Communicates the best move found by the engine
    BestMove,
    /// Initialization complete
    InitializationComplete,
}

pub struct Engine {
    /// A `JoinHandle` for the engine thread.
    handle: JoinHandle<()>,
    /// A `Sender` to transmit commands into the engine thread.
    tx: Sender<Command>,
}

impl Engine {
    pub fn new(session_tx: Sender<Message>) -> Self {
        // A channel to send commands into the engine thread.
        let (tx, rx) = unbounded::<Command>();

        let handle = thread::spawn(move || {
            // let mut pos = Position::start_pos();
            // let turn = pos.turn().clone();
            // let mut searcher = PVSearch::new(pos);
            // let val = searcher.iterative_deepening(19) * if turn.is_white() { 1 } else { -1 };

            // loop {
            //     // TODO: handle the error case
            //     if let Ok(cmd) = rx.recv() {
            //         match cmd {
            //             Command::Initialize => {
            //                 core::init::init_globals();
            //                 // TODO: use result
            //                 session_tx.send(Message::FromEngine(Report::InitializationComplete));
            //             }
            //         }
            //     }
            // }

            // // -------------------

            let mut quit = false;
            let mut halt = true;

            let mut engine_inner = EngineInner::new(session_tx);

            // Keep the thread alive until we receive a quit command
            while !quit {
                let cmd = rx.recv().expect("Error: fatal");

                match cmd {
                    Command::Initialize => engine_inner.init(),
                    Command::SetPosition => todo!(),
                    Command::Halt => halt = true,
                    Command::Quit => quit = true,
                    Command::Search => todo!(),
                }
            }
        });

        Self { handle, tx }
    }

    pub fn send(&self, cmd: Command) {
        // TODO: use the result
        self.tx.send(cmd);
    }
}

/// A convenient way to organise the code which runs in the engine thread.
/// Otherwise we would just have a load of local variables to manage.
struct EngineInner {
    /// A `Sender` to emit `Message`s back to the `Session`.
    session_tx: Sender<Message>,
}

impl EngineInner {
    pub fn new(session_tx: Sender<Message>) -> Self {
        Self { session_tx }
    }

    pub fn init(&self) {
        core::init::init_globals();
        self.send(Report::InitializationComplete);
    }

    pub fn send(&self, report: Report) {
        self.session_tx.send(Message::FromEngine(report));
    }
}
