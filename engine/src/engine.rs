use crate::search::pv_search::PVSearch;
use crate::sess::Message;
use crate::uci::Pos;
use core::position::Position;

use crossbeam_channel::{unbounded, Sender};

use std::thread::{self, JoinHandle};

#[derive(Debug)]
pub enum Command {
    Initialize,
    SetPosition(Pos),
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

/// Owns the thread in which the chess engine's search routine executes, and passes
/// commands for the engine into that thread through a channel.
pub struct Engine {
    /// A `JoinHandle` for the engine thread.
    handle: Option<JoinHandle<()>>,
    /// A `Sender` to transmit commands into the engine thread.
    tx: Sender<Command>,
}

impl Engine {
    pub fn new(session_tx: Sender<Message>) -> Self {
        // A channel to send commands into the engine thread.
        let (tx, rx) = unbounded::<Command>();

        let engine_thread = thread::spawn(move || {
            let mut quit = false;
            let mut halt = true;

            let mut engine_inner = EngineInner::new(session_tx);

            // Keep the thread alive until we receive a quit command
            while !quit {
                let cmd = rx.recv().expect("Error: fatal");

                match cmd {
                    Command::Initialize => engine_inner.init(),
                    Command::SetPosition(pos) => engine_inner.set_position(pos),
                    Command::Halt => halt = true,
                    Command::Quit => quit = true,
                    Command::Search => todo!(),
                }

                // If the engine isn't halted, and we aren't quitting, proceed
                // to run the search.
                if !halt && !quit {}
            }
        });

        Self {
            handle: Some(engine_thread),
            tx,
        }
    }

    /// Send a `Command` into the engine thread.
    pub fn send(&self, cmd: Command) {
        // TODO: use the result
        self.tx.send(cmd);
    }

    /// When quitting a session, use this to join on the `Engine` thread and wait
    /// for it to successfully shutdown.
    pub fn wait_for_shutdown(&mut self) {
        if let Some(h) = self.handle.take() {
            h.join().expect("Error: fatal");
        }
    }
}

/// A convenient way to organise the code which runs in the engine thread.
/// Otherwise we would just have a load of local variables to manage.
struct EngineInner {
    /// A `Sender` to emit `Message`s back to the `Session`.
    session_tx: Sender<Message>,
    /// The internal board position.
    pos: Option<Position>,
}

impl EngineInner {
    pub fn new(session_tx: Sender<Message>) -> Self {
        Self {
            session_tx,
            pos: None,
        }
    }

    pub fn init(&mut self) {
        // Ensure globals variables like magic numbers have been initialized.
        core::init::init_globals();

        // Set up the initial position on the internal board.
        self.pos = Some(Position::start_pos());

        // Report that initialization has completed.
        self.report(Report::InitializationComplete);
    }

    pub fn set_position(&mut self, pos: Pos) {
        match pos {
            Pos::Startpos => todo!("engine will set the starting position on its internal board"),
            Pos::Fen(fen) => todo!(
                "engine will set the position on its internal board: {}",
                fen
            ),
        }
    }

    pub fn report(&self, report: Report) {
        self.session_tx.send(Message::FromEngine(report));
    }
}
