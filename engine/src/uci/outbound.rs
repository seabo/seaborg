use super::Uci;
use crate::engine::Info;

use log::info;

static NAME: &str = "seaborg";
static VERSION: &str = "0.1.0";
static AUTHORS: &str = "George Seabridge <georgeseabridge@gmail.com>";

/// Represents a response to be sent to the GUI.
#[derive(Clone, Debug, PartialEq)]
pub enum Res {
    Uciok,
    Readyok,
    Identify,
    BestMove(String),
    Info(Info),
    Quit,
    Error(String),
}

impl std::fmt::Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Res::Uciok => writeln!(f, "uciok"),
            Res::Readyok => writeln!(f, "readyok"),
            Res::Identify => {
                writeln!(f, "id name {} {}", NAME, VERSION);
                writeln!(f, "id author {}", AUTHORS)
            }
            Res::BestMove(uci_move) => writeln!(f, "bestmove {}", uci_move),
            Res::Info(info) => writeln!(
                f,
                "info depth {} seldepth {} score cp {} nodes {} nps {} pv {}",
                info.depth, info.seldepth, info.score, info.nodes, info.nps, info.pv
            ),
            Res::Quit => writeln!(f, "exiting"),
            Res::Error(msg) => writeln!(f, "err: {}", msg),
        }
    }
}

/// Functions to emit uci responses to stdout
impl Uci {
    pub fn emit(res: Res) {
        info!("writing response to stdout: {:?}", res);
        print!("{}", res);
    }
}
