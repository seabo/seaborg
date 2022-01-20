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

use crate::sess::Message;
use crate::uci::{Req, Res, Uci};

use crossbeam_channel::{unbounded, Sender};
use std::thread::{self, JoinHandle};

pub struct Comm {
    /// Join handle for the thread which listens for GUI input. This thread sends
    /// messages back to the `Session` through a channel whose `Receiver` is stored
    /// on `Session.rx`.
    from_gui: Option<JoinHandle<()>>,
    /// Join handle for the thread which receives engine reports and sends messages
    /// back to the GUI via stdout.
    to_gui: Option<JoinHandle<()>>,
    /// Transmitter for sending messages to the `to_gui` thread. When the `to_gui`
    /// thread receives these messages, it writes the appropriate output to stdout.
    to_gui_tx: Sender<Res>,
}

impl Comm {
    /// Bootstrap a fresh engine session and return it.
    pub fn new(from_gui_tx: Sender<Message>) -> Self {
        let from_gui = Self::from_gui_thread(from_gui_tx);
        let (to_gui, to_gui_tx) = Self::to_gui_thread();
        Self {
            from_gui: Some(from_gui),
            to_gui: Some(to_gui),
            to_gui_tx,
        }
    }

    pub fn send(&self, msg: Res) {
        // TODO: use result
        self.to_gui_tx.send(msg);
    }

    /// When quitting a session, use this to join on the `Comm` threads and wait for
    /// them to successfully shutdown.
    pub fn wait_for_shutdown(&mut self) {
        if let Some(h) = self.from_gui.take() {
            h.join().expect("Error: fatal");
        }

        if let Some(h) = self.to_gui.take() {
            h.join().expect("Error: fatal");
        }
    }

    fn from_gui_thread(tx: Sender<Message>) -> JoinHandle<()> {
        let gui_in = thread::spawn(move || loop {
            // Blocks until a new message is received on stdin.
            let uci_msg = Uci::read_command();

            // TODO: use the result
            tx.send(Message::FromGui(uci_msg.clone()));

            if uci_msg == Req::Quit {
                break;
            }
        });

        gui_in
    }

    fn to_gui_thread() -> (JoinHandle<()>, Sender<Res>) {
        let (tx, rx) = unbounded::<Res>();

        let to_gui_thread = thread::spawn(move || loop {
            let engine_cmd = rx.recv();

            match engine_cmd {
                Ok(res) => Uci::emit(res),
                Err(_) => {}
            }

            if let Ok(Res::Quit) = engine_cmd {
                break;
            }
        });

        (to_gui_thread, tx)
    }
}
