use super::{Req, Uci};
use std::io;

impl Uci {
    /// Blocking method which awaits the next command to stdin.
    pub fn read_command() -> Req {
        let mut buf = String::new();

        // TODO: use the `Result`.
        io::stdin().read_line(&mut buf);

        match Uci::parse(buf) {
            Ok(msg) => msg,
            Err(err) => {
                err.emit();
                Self::read_command()
            }
        }
    }
}
