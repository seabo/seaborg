use crate::search::params::{Builder, BuilderError, BuilderResult, Params};
use crate::search::search::{Search, TimingMode};
use crate::sess::Message;
use crate::uci::Pos;

use crossbeam_channel::{unbounded, Sender};
use log::info;

use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

#[derive(Debug)]
pub enum Command {
    Initialize,
    SetPosition((Pos, Option<Vec<String>>)),
    Search(TimingMode),
    Quit,
    Display,
}

#[derive(Clone, Debug)]
/// Configuration options for the engine.
pub enum EngineOpt {
    /// Whether or not the transposition table is turned on.
    TranspositionTable(bool),
    /// Whether or not iterative deepening is being used.
    IterativeDeepening(bool),
    /// Whether or not move ordering is being used.
    MoveOrdering(bool),
}

/// Represents an engine report. This is passed back from the `Engine` to the
/// `Session`, which then forwards it to the `Comm` module via `comm.send()`.
/// The `Comm` module then takes responsibility for handling the report, usually
/// by converting into a uci response written to stdout.
// TODO: this should live in a more appropriate module eventually.
pub enum Report {
    /// Communicates the best move found by the engine.
    BestMove(String),
    /// Info
    Info(Info),
    /// Initialization complete.
    InitializationComplete,
    /// Error report.
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub(crate) depth: u8,
    pub(crate) seldepth: u8,
    pub(crate) score: i32,
    pub(crate) nodes: usize,
    pub(crate) nps: usize,
    pub(crate) pv: String,
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
    /// Flag which allows the search process to know if it needs to halt while
    /// in the middle of a search.
    ///
    /// We pass an `Arc` clone of this to the `Search`, and then inside
    /// the main search loop, we intermittently check that the value hasn't recently
    /// been set to `true`. When it does get set to `true` the search can bail early
    /// and send useful data back through the channel.
    halt: Arc<RwLock<bool>>,
}

impl Engine {
    pub fn new(session_tx: Sender<Message>) -> Self {
        // A channel to send commands into the engine thread.
        let (tx, rx) = unbounded::<Command>();

        // A halt flag.
        let halt = Arc::new(RwLock::new(false));
        let halt_clone = Arc::clone(&halt);

        let engine_thread = thread::spawn(move || {
            let mut quit = false;
            let mut engine_inner = EngineInner::new(session_tx);

            // Keep the thread alive until we receive a quit command
            while !quit {
                let cmd = rx
                    .recv()
                    .expect("engine thread unable to receive communications from session thread");

                match cmd {
                    Command::Initialize => engine_inner.init(),
                    Command::SetPosition((pos, moves)) => engine_inner.set_position(pos, moves),
                    Command::Search(mode) => {
                        engine_inner.set_search_mode(mode);
                        engine_inner.search(Arc::clone(&halt_clone));
                    }
                    Command::Quit => quit = true,
                    Command::Display => {
                        engine_inner.display();
                    }
                }
            }
        });

        Self {
            handle: Some(engine_thread),
            halt,
            tx,
        }
    }

    /// Send a `Command` into the engine thread.
    pub fn send(&self, cmd: Command) {
        self.tx
            .send(cmd)
            .expect("session thread failed to send command into engine thread");
    }

    /// Halt the search process.
    pub fn halt(&self) {
        *self.halt.write().unwrap() = true;
    }

    /// Unhalt the search process.
    pub fn unhalt(&self) {
        *self.halt.write().unwrap() = false;
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

    pub fn set_position(&mut self, pos: Pos, moves: Option<Vec<String>>) {
        let result = self.builder.set_position(pos, moves);

        self.handle_result(result);
    }

    /// Display the position in ascii.
    pub fn display(&self) {
        match self.builder.pos() {
            Some(pos) => pos.pretty_print(),
            None => {}
        }
    }

    pub fn set_search_mode(&mut self, search_mode: TimingMode) {
        info!("setting engine search mode to: {:?}", search_mode);

        self.builder
            .set_search_mode(search_mode)
            .expect("couldn't set the search mode");
    }

    /// Launch the search. This will take the current params `Builder` and
    /// build the actual `Params` struct. Then a new `Search` will be started
    /// with those `Params`.
    pub fn search(&mut self, halt: Arc<RwLock<bool>>) {
        // Ensure globals variables like magic numbers have been initialized.
        core::init::init_globals();
        // Build the search params.
        let params = self.builder.clone().into();

        let mut search = Search::new(params, Some(self.session_tx.clone()), halt);

        // TODO: for now, the way this is implemented the `Search` will be
        // dropped as soon as this returns. Ideally we want to keep it around
        // and allow a paused search to be restarted. This probably just means
        // set the `Search` as a field on `EngineInner`.
        let val = search.iterative_deepening();
    }

    fn handle_result(&self, res: BuilderResult) {
        match res {
            Ok(_) => {}
            Err(be) => match be {
                BuilderError::IllegalFen(fe) => {
                    self.report_error(format!("illegal FEN string: {}", fe));
                }
                BuilderError::IllegalMove(mov) => {
                    self.report_error(format!("illegal move: {}", mov));
                }
            },
        }
    }

    pub fn report(&self, report: Report) {
        self.session_tx
            .send(Message::FromEngine(report))
            .expect("couldn't send engine report to session thread");
    }

    pub fn report_error(&self, msg: String) {
        self.report(Report::Error(msg));
    }
}
