use super::{Req, Uci};
use std::fs;
use std::io::{self, Write};

// TODO: this doesn't make sense as part of UCI. Reading from the command
// line should go elsewhere.
impl Uci {
    /// Blocking method which awaits the next command to stdin.
    pub fn read_command() -> Req {
        let mut buf = String::new();

        io::stdin().read_line(&mut buf).expect("couldn't read line");

        Uci::log(&buf);

        match Uci::parse(buf) {
            Ok(msg) => msg,
            Err(err) => {
                err.emit();
                Self::read_command()
            }
        }
    }

    fn log(buf: &str) {
        let path = "../../log.txt";
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .expect("Unable to open file");
        file.write_all(buf.as_bytes())
            .expect("Unable to write data");
    }
}
