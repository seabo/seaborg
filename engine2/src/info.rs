//! An engine info report.
use super::score::Score;

/// A UCI info string.
#[derive(Debug)]
pub struct Info {
    pub(super) depth: u8,
    pub(super) time: usize,
    pub(super) nodes: usize,
    pub(super) pv: String,
    pub(super) score: Score,
    pub(super) hashfull: u16,
    pub(super) nps: u32,
}

impl std::fmt::Display for Info {
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
        write!(f, "pv {}", self.pv)?;
        writeln!(f)
    }
}
