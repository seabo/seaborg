// use super::engine::Engine;
use super::info::Info;
use super::uci::{self, Command};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::{io, thread};

/// A response to the GUI.
#[derive(Debug)]
pub enum Resp {
    Uciok,
    Id,
    OptionsList,
    ReadyOk,
    Info(Info),
    BestMove,
    UciParseError(uci::Error),
}

/// Represents a session with the engine. This allows the user or GUI, to communicate with the
/// engine using UCI, manipulate the internal settings and run the search.
pub struct Session {
    /// Channel for transmitting message from Session thread to Engine thread.
    sess_to_eng: (Sender<uci::Command>, Receiver<uci::Command>),
    /// Channel for transmitting message from Engine thread to Session thread.
    eng_to_sess: (Sender<Resp>, Receiver<Resp>),
    /// Channel for transmitting stdin messages from the dedicated stdin thread to the Session
    /// thread.
    stdin_to_sess: (Sender<String>, Receiver<String>),
    /// A separate channel for informing the search process to stop immediately. This has capacity
    /// one, and sends no data beyond `()` to inform `Search` that it should stop.
    stop_search: (Sender<()>, Receiver<()>),
}

impl Session {
    pub fn new() -> Self {
        Self {
            sess_to_eng: unbounded::<uci::Command>(),
            eng_to_sess: unbounded::<Resp>(),
            stdin_to_sess: unbounded::<String>(),
            stop_search: bounded::<()>(1),
        }
    }

    pub fn launch(&mut self) {
        let tx = self.eng_to_sess.0.clone();
        let rx = self.sess_to_eng.1.clone();
        let rx_search = self.stop_search.1.clone();

        let stdin_tx = self.stdin_to_sess.0.clone();

        // Launch the stdin thread.
        thread::spawn(move || {
            // Buffer to read in UCI commands from std in.
            let mut buf: String = String::with_capacity(128);
            loop {
                buf.clear();
                io::stdin().read_line(&mut buf).expect("couldn't read line");

                let _ = stdin_tx.send(buf.clone());
            }
        });

        // // Launch the engine thread.
        // thread::spawn(move || {
        //     let mut engine = Engine::new(tx, rx, rx_search);
        //     engine.launch();
        // });

        // loop {
        //     // In each loop cycle, we check for any input from the GUI in `poll_input`. If there is
        //     // anything, we parse it and transmit the relevant command into the engine.
        //     self.poll_input();

        //     // Next, we check to see if the engine has sent any messages or reports. These are
        //     // printed to stdout.
        //     self.poll_output();
        // }
    }

    fn poll_input(&mut self) {
        match self.stdin_to_sess.1.try_recv() {
            Ok(s) => match uci::Parser::parse(&s) {
                Ok(cmd) => match cmd {
                    Command::Stop => {
                        let _ = self.stop_search.0.send(());
                    }
                    Command::Quit => {
                        let _ = self.stop_search.0.send(());
                    }
                    _ => {
                        let _ = self.sess_to_eng.0.send(cmd);
                    }
                },
                Err(err) => eprintln!("error: {:?}", err),
            },
            Err(_) => {}
        }
    }

    fn poll_output(&mut self) {
        match self.eng_to_sess.1.try_recv() {
            Ok(resp) => self.dispatch_response(resp),
            Err(_) => {}
        }
    }

    fn dispatch_response(&mut self, resp: Resp) {
        match resp {
            Resp::Uciok => Self::uciok(),
            Resp::Id => Self::id(),
            Resp::OptionsList => Self::options_list(),
            Resp::ReadyOk => Self::readyok(),
            Resp::Info(i) => Self::report_info(i),
            Resp::BestMove => todo!(),
            Resp::UciParseError(err) => Self::uci_parse_error(err),
        }
    }

    fn uciok() {
        println!("uciok");
    }

    fn id() {
        println!("id name Seaborg 0.1.1");
        println!("id author George Seabridge")
    }

    fn options_list() {
        println!("option name Hash type spin default 32 min 0 max 4096");
    }

    fn readyok() {
        println!("readyok")
    }

    fn uci_parse_error(err: uci::Error) {
        println!("{:?}", err);
    }

    fn report_info(i: Info) {
        println!("{}", i);
    }
}
