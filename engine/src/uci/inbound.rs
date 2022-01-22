use super::{Req, Uci};

use log::info;

use std::io;

// TODO: this doesn't make sense as part of UCI. Reading from the command
// line should go elsewhere.
impl Uci {
    /// Blocking method which awaits the next command to stdin.
    pub fn read_command() -> Req {
        let mut buf = String::new();

        io::stdin().read_line(&mut buf).expect("couldn't read line");

        info!("command received via stdin: {}", &buf[..buf.len() - 1]);

        match Uci::parse(buf) {
            Ok(msg) => msg,
            Err(err) => {
                err.emit();
                Self::read_command()
            }
        }
    }
}
