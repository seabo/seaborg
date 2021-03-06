use crate::search::search::SearchMode;
use crate::uci::Pos;
use core::position::{FenError, Position};

/// Default transposition table capacity. This means the table will have `2^27`
/// available slots, which is approximately 134 million.
static DEFAULT_TT_CAP: u32 = 27;
/// Default search mode to use.
static DEFAULT_SEARCH_MODE: SearchMode = SearchMode::Infinite;

/// The parameters to be used to run a search.
#[derive(Clone, Debug)]
pub struct Params {
    /// The `Position` to search.
    pos: Position,
    /// The capacity to use for the transposition table. The number of available
    /// transposition table entries will be `2^tt_cap`.
    pub tt_cap: u32,
    /// The search mode to use.
    pub search_mode: SearchMode,
    // - search type (iterative deepening, fixed depth)
}

impl Params {
    pub fn take_pos(&mut self) -> Position {
        std::mem::replace(&mut self.pos, Position::blank())
    }
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
    /// The search mode to use.
    search_mode: Option<SearchMode>,
}

impl Builder {
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the position.
    pub fn set_position(&mut self, pos: Pos, moves: Option<Vec<String>>) -> BuilderResult {
        // If we are being asked to set a position, then we need to ensure that
        // the global variables are initialised. This is inexpensive if initialization
        // has already taken place, which should be the case in any normal UCI
        // interaction, because the GUI is supposed to ask `isready` before
        // any further position setup, and we run `init_globals()` after receiving
        // `isready`.
        core::init::init_globals();

        let mut position = match pos {
            Pos::Startpos => Position::start_pos(),
            Pos::Fen(fen) => Position::from_fen(&fen)?,
        };

        match moves {
            Some(move_list) => {
                for mov in move_list {
                    match position.make_uci_move(&mov) {
                        Some(_) => {}
                        None => return Err(BuilderError::IllegalMove(mov)),
                    }
                }
            }
            None => {}
        }

        self.pos = Some(position);

        Ok(())
    }

    /// Set the transposition table capacity.
    ///
    /// The number of available transposition tabel entries will be `2^tt_cap`.
    pub fn set_tt_cap(&mut self, cap: u32) -> BuilderResult {
        self.tt_cap = Some(cap);
        Ok(())
    }

    pub fn set_search_mode(&mut self, search_mode: SearchMode) -> BuilderResult {
        self.search_mode = Some(search_mode);
        Ok(())
    }

    pub fn build(self) -> Params {
        let pos = match self.pos {
            Some(pos) => pos,
            None => Default::default(),
        };

        let tt_cap = match &self.tt_cap {
            Some(tt_cap) => *tt_cap,
            None => DEFAULT_TT_CAP,
        };

        let search_mode = match &self.search_mode {
            Some(search_mode) => *search_mode,
            None => DEFAULT_SEARCH_MODE,
        };

        Params {
            pos,
            tt_cap,
            search_mode,
        }
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            pos: None,
            tt_cap: None,
            search_mode: None,
        }
    }
}

pub type BuilderResult = Result<(), BuilderError>;

#[derive(Debug)]
pub enum BuilderError {
    IllegalFen(FenError),
    IllegalMove(String),
}

impl From<FenError> for BuilderError {
    fn from(fe: FenError) -> Self {
        BuilderError::IllegalFen(fe)
    }
}
