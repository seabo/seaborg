use crate::uci::Pos;
use core::position::{FenError, Position};

/// The parameters to be used to run a search.
#[derive(Clone, Debug)]
pub struct Params {
    /// The `Position` to search.
    pos: Position,
    /// The capacity to use for the transposition table. The number of available
    /// transposition table entries will be `2^tt_cap`.
    tt_cap: u32,
    // TODO: much more will go here, for example:
    // - time management
    // - search type (iterative deepening, fixed depth)
}

/// A helper structure to construct `Params` with defaults and methods
/// for adding new options as they are sent by the GUI.
#[derive(Clone, Debug)]
pub struct Builder {
    /// The `Position` to search.
    pos: Option<Position>,
    /// The capacity to use for the transposition table. The number of available
    /// transposition table entries will be `2^tt_cap`.
    tt_cap: Option<u32>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            pos: None,
            tt_cap: None,
        }
    }

    pub fn set_position(&mut self, pos: Pos) -> BuilderResult {
        // If we are being asked to set a position, then we need to ensure that
        // the global variables are initialised. This is inexpensive if initialization
        // has already taken place, which should be the case in any normal UCI
        // interaction, because the GUI is supposed to ask `isready` before
        // any further position setup, and we run `init_globals()` after receiving
        // `isready`.
        core::init::init_globals();

        match pos {
            Pos::Startpos => self.pos = Some(Position::start_pos()),
            Pos::Fen(fen) => {
                let pos = Position::from_fen(&fen)?;
                self.pos = Some(pos);
            }
        }
        Ok(())
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            pos: None,
            tt_cap: None,
        }
    }
}

pub type BuilderResult = Result<(), BuilderError>;

pub enum BuilderError {
    IllegalFen(FenError),
}

impl From<FenError> for BuilderError {
    fn from(fe: FenError) -> Self {
        BuilderError::IllegalFen(fe)
    }
}
