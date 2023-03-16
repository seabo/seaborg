use super::options::{Config, EngineOpt};
use super::search::Search;
use super::session::Resp;
use super::time::TimingMode;
use super::tt::Table;
use super::uci::{Command, Error};
use core::position::Position;

use crossbeam_channel::{Receiver, Sender};

use std::env;
use std::sync::{Arc, Mutex};

/// Manages the search and related configuration. This runs in a separate thread from the main
/// process.
pub struct Engine {
    /// Transmitter of messages to the Session thread.
    pub(super) tx: Sender<Resp>,
    /// Receiver of messages from the Session thread.
    pub(super) rx: Receiver<Command>,
    /// Current configuration of the engine.
    pub(super) config: Arc<Mutex<Config>>,
    /// The internal board position.
    pub(super) search: Search,
}

impl Engine {
    pub fn new(tx: Sender<Resp>, rx: Receiver<Command>, rx_search: Receiver<()>) -> Self {
        // Since we are creating the engine, which includes a `Position`, we need to ensure that
        // the globals are initialised first. This is inexpensive if it has already been called
        // elsewhere.
        core::init::init_globals();

        let search_tx = tx.clone();
        let config: Arc<Mutex<Config>> = Default::default();

        match env::var("SEABORG_DEBUG") {
            Ok(v) => {
                if v == "true" || v == "True" {
                    match config.lock() {
                        Ok(mut c) => c.set_option(EngineOpt::DebugMode(true)),
                        _ => {}
                    }
                }
            }
            Err(_) => {}
        }

        Self {
            tx,
            rx,
            config: config.clone(),
            search: Search::new_with_channels(Position::start_pos(), config, search_tx, rx_search),
        }
    }

    pub fn launch(&mut self) {
        loop {
            let s = self.rx.recv().unwrap();
            self.dispatch_command(s);
        }
    }

    fn dispatch_command(&mut self, cmd: Command) {
        match cmd {
            Command::Uci => self.command_uci(),
            Command::IsReady => self.command_isready(),
            Command::UciNewGame => self.command_ucinewgame(),
            Command::SetPosition((p, m)) => self.command_set_position(p, m),
            Command::SetOption(o) => self.command_set_option(o),
            Command::Go(tm) => self.command_go(tm),
            Command::Stop => todo!(),
            Command::Quit => todo!(),
            Command::Display => self.command_display(),
            Command::Config => self.command_config(),
            Command::Perft(d) => self.command_perft(d),
        }
    }

    fn command_uci(&mut self) {
        self.report(Resp::Id);
        self.report(Resp::OptionsList);
        self.report(Resp::Uciok);
    }

    fn command_display(&self) {
        self.search.pos.pretty_print();
    }

    fn command_config(&self) {
        match self.config.lock() {
            Ok(c) => println!("{:#?}", c),
            Err(_) => println!("config error"),
        }
    }

    fn command_isready(&self) {
        let _ = self.tx.send(Resp::ReadyOk);
    }

    fn command_ucinewgame(&self) {}

    fn command_set_position(&mut self, pos: String, moves: Vec<String>) {
        match Position::from_fen(&pos) {
            Ok(mut pos) => {
                for mov in moves {
                    if pos.make_uci_move(&mov).is_none() {
                        let _ = self.tx.send(Resp::UciParseError(Error::InvalidMove));
                    }
                }

                self.search.pos = pos
            }
            Err(err) => self.report(Resp::UciParseError(Error::InvalidPosition(err))),
        }
    }

    fn command_set_option(&mut self, o: EngineOpt) {
        match self.config.lock() {
            Ok(mut c) => c.set_option(o),
            _ => {}
        }
    }

    fn command_go(&mut self, tm: TimingMode) {
        let _score = self.search.start_search(tm);
    }

    fn command_perft(&mut self, d: usize) {
        super::perft::Perft::divide(&mut self.search.pos, d, true, false);
    }

    fn report(&mut self, resp: Resp) {
        let _ = self.tx.send(resp);
    }
}
