//! An engine info report.
use super::score::Score;
use core::mov::Move;

/// A UCI info report.
#[derive(Debug)]
pub enum Info {
    Pv(PvInfo),
    CurrMove(CurrMoveInfo),
}

impl std::fmt::Display for Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Info::*;
        match self {
            Pv(i) => i.fmt(f),
            CurrMove(i) => i.fmt(f),
        }
    }
}

/// A UCI PV report.
///
/// These are usually issued at the end of each iterative deepening iteration.
#[derive(Debug)]
pub struct PvInfo {
    pub(super) depth: u8,
    pub(super) time: usize,
    pub(super) nodes: usize,
    pub(super) pv: String,
    pub(super) score: Score,
    pub(super) hashfull: u16,
    pub(super) nps: u32,
}

impl std::fmt::Display for PvInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "info ")?;
        write!(f, "depth {} ", self.depth)?;
        // write!(f, "seldepth {}", self.seldepth)?; // TODO
        write!(f, "multipv 1 ")?; // TODO: we don't have an option to send further PVs, so always
                                  // send this.
        write!(f, "score {} ", self.score)?;
        write!(f, "nodes {} ", self.nodes)?;
        write!(f, "nps {} ", self.nps)?;
        write!(f, "hashfull {} ", self.hashfull)?;
        write!(f, "time {} ", self.time)?;
        write!(f, "pv {}", self.pv)
    }
}

/// A UCI current move report.
#[derive(Debug)]
pub struct CurrMoveInfo {
    pub(super) depth: u8,
    pub(super) currmove: Move,
    pub(super) number: u8,
}

impl std::fmt::Display for CurrMoveInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "info ")?;
        write!(f, "depth {} ", self.depth)?;
        write!(f, "currmove {} ", self.currmove)?;
        write!(f, "currmovenumber {} ", self.number)
    }
}
