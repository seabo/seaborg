use super::Uci;

static NAME: &str = "rchess";
static VERSION: &str = "0.1.0";
static AUTHORS: &str = "George Seabridge <georgeseabridge@gmail.com>";

/// Represents a response to be sent to the GUI.
#[derive(Clone, Debug, PartialEq)]
pub enum Res {
    Uciok,
    Readyok,
    Identify,
    Quit,
    Error(String),
}

/// Functions to emit uci responses to stdout
impl Uci {
    pub fn identify() {
        println!("id name {} {}", NAME, VERSION);
        println!("id author {}", AUTHORS);
    }

    pub fn emit(res: Res) {
        match res {
            Res::Uciok => println!("uciok"),
            Res::Readyok => println!("readyok"),
            Res::Identify => Self::identify(),
            Res::Quit => println!("exiting"),
            Res::Error(msg) => println!("{}", msg),
        }
    }
}
