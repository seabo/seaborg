use crate::search::params::{Builder, BuilderError, BuilderResult, Params};
use crate::search::search::Search;
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
    /// Communicates the best move found by the engine.
    BestMove,
    /// Initialization complete.
    InitializationComplete,
    /// Error report.
    Error(String),
}

/// Owns the thread in which the chess engine's search routine executes, and passes
/// commands for the engine into that thread through a channel.
///
/// This struct lives in the `Session` thread.
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
                    Command::Search => engine_inner.search(),
                    Command::Halt => halt = true,
                    Command::Quit => quit = true,
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
///
/// This struct lives in the `Engine` thread.
struct EngineInner {
    /// A `Sender` to emit `Message`s back to the `Session`.
    session_tx: Sender<Message>,
    /// Helper to construct search `Params` structs for each new search.
    builder: Builder,
}

impl EngineInner {
    pub fn new(session_tx: Sender<Message>) -> Self {
        Self {
            session_tx,
            builder: Builder::default(),
        }
    }

    pub fn init(&mut self) {
        // Ensure globals variables like magic numbers have been initialized.
        core::init::init_globals();

        // Report that initialization has completed.
        self.report(Report::InitializationComplete);
    }

    pub fn set_position(&mut self, pos: Pos) {
        let result = self.builder.set_position(pos);

        self.handle_result(result);
    }

    /// Launch the search. This will take the current params `Builder` and
    /// build the actual `Params` struct. Then a new `Search` will be started
    /// with those `Params`.
    pub fn search(&mut self) {
        let params = std::mem::take(&mut self.builder).build();

        let mut search = Search::new(params);

        // TODO: for now, the way this is implemented the `Search` will be
        // dropped as soon as this returns. Ideally we want to keep it around
        // and allow a paused search to be restarted. This probably just means
        // set the `Search` as a field on `EngineInner`.
        let val = search.iterative_deepening(5);
        println!("search yielded {}", val);
    }

    fn handle_result(&self, res: BuilderResult) {
        match res {
            Ok(()) => {}
            Err(be) => match be {
                BuilderError::IllegalFen(fe) => {
                    self.report_error(format!("{}", fe));
                }
            },
        }
    }

    pub fn report(&self, report: Report) {
        // TODO: use the result.
        self.session_tx.send(Message::FromEngine(report));
    }

    pub fn report_error(&self, msg: String) {
        self.report(Report::Error(msg));
    }
}
