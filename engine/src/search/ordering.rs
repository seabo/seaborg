use crate::search::search::TTData;
use crate::tables::Table;

use core::mov::Move;
use core::movelist::{MoveList, MAX_MOVES};
use core::position::Position;

use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

const MVV_LVA_OFFSET: u16 = 100;
const KILLER_VALUE_NEWEST: u16 = 30;
const KILLER_VALUE_OLDEST: u16 = 20;
const QUIET_VALUE: u16 = 10;

// MVV_LVA[victim][attacker]
pub const MVV_LVA: [[u16; 7]; 7] = [
    [0, 0, 0, 0, 0, 0, 0],      // victim None, attacker K, Q, R, B, N, P, None
    [0, 15, 14, 13, 12, 11, 0], // victim P, attacker K, Q, R, B, N, P, None
    [0, 25, 24, 23, 22, 21, 0], // victim N, attacker K, Q, R, B, N, P, None
    [0, 35, 34, 33, 32, 31, 0], // victim B, attacker K, Q, R, B, N, P, None
    [0, 45, 44, 43, 42, 41, 0], // victim R, attacker K, Q, R, B, N, P, None
    [0, 55, 54, 53, 52, 51, 0], // victim Q, attacker K, Q, R, B, N, P, None
    [0, 0, 0, 0, 0, 0, 0],      // victim K, attacker K, Q, R, B, N, P, None
];

/// A wrapper around a `MoveList`.
///
/// Implements `Iterator` and uses a 'selection sort' style algorithm
/// to return `Move`s in a priority ordering.
///
/// An `OrderedMoveList` consumes the underlying `MoveList`, so it won't
/// be available after the iteration.
pub struct OrderedMoveList {
    /// A reference to the `Position` struct associated with this `OrderedMoveList`.
    pos: Rc<RefCell<Position>>,
    /// A reference to the transposition table associated with this `OrderedMoveList`.
    tt: Rc<RefCell<Table<TTData>>>,
    /// The underlying `MoveList`. This gets consumed by the `OrderedMoveList`
    /// and won't be available after the iteration.
    pub move_list: Option<MoveList>,
    /// Parallel array to the `move_list`, containing ordering scores of the associated moves.
    move_scores: Option<[u16; MAX_MOVES]>,
    /// Boolean flag to indicate if we have prepared for return the full move list yet.
    prepared: bool,
    /// The transposition table move.
    tt_move: Option<Move>,
    /// THe killer moves.
    killers: (Option<Move>, Option<Move>),
    /// Tracks whether we have yielded the transposition table move yet
    yielded_tt_move: bool,
    /// Tracks how many quiet moves (in the `Rest` ordering phase) have been yielded.
    quiet_moves_yielded: u8,
}

impl OrderedMoveList {
    pub fn new(
        pos: Rc<RefCell<Position>>,
        tt: Rc<RefCell<Table<TTData>>>,
        killers: (Option<Move>, Option<Move>),
    ) -> Self {
        let mut list = Self {
            pos,
            tt,
            move_list: None,
            move_scores: None,
            prepared: false,
            tt_move: None,
            killers,
            yielded_tt_move: false,
            quiet_moves_yielded: 0,
        };

        let tt_move = list.get_tt_move();
        list.tt_move = tt_move;

        list
    }

    pub fn tt_move(&self) -> Option<Move> {
        self.tt_move
    }

    fn pos(&self) -> Ref<'_, Position> {
        self.pos.borrow()
    }

    fn tt(&self) -> Ref<'_, Table<TTData>> {
        self.tt.borrow()
    }

    fn get_tt_move(&self) -> Option<Move> {
        match self.tt().get(&self.pos()) {
            Some(tt_entry) => Some(tt_entry.best_move()),
            None => None,
        }
    }

    /// Assigns a score to the remaining moves after having already returned the TT move.
    fn score_move(&self, mov: Move) -> u16 {
        if mov.is_null() {
            0
        } else if mov.is_capture() {
            // Note: the killer moves should never be captures, so there is no overlap.
            let pos = self.pos();
            let victim = pos.piece_at_sq(mov.dest()).type_of() as usize;
            let attacker = pos.piece_at_sq(mov.orig()).type_of() as usize;
            MVV_LVA_OFFSET + MVV_LVA[victim][attacker]
        } else if Some(mov) == self.killers.0 {
            KILLER_VALUE_OLDEST
        } else if Some(mov) == self.killers.1 {
            KILLER_VALUE_NEWEST
        } else {
            QUIET_VALUE
        }
    }

    fn prepare_move_list(&mut self) {
        let moves = self.pos().generate_moves();
        self.move_list = Some(moves);

        // Build a structure with scores for each move in the list.
        let mut move_scores = [0; MAX_MOVES];

        for (i, mov) in self.move_list.as_ref().unwrap().iter().enumerate() {
            // Remove the tt move as we'll already have returned it by now.
            if Some(*mov) == self.tt_move {
                unsafe {
                    *move_scores.get_unchecked_mut(i) = 0;
                }
            }
            let score = self.score_move(*mov);
            unsafe {
                *move_scores.get_unchecked_mut(i) = score;
            }
        }

        self.move_scores = Some(move_scores);

        // TODO: once killer moves are implemented, remove them from the list
        // killer moves as we'll already be returning them before getting this far.

        self.prepared = true;
    }

    fn yield_next(&mut self) -> Option<(Move, OrderingPhase)> {
        // Perform a selection sort on the move list and return the highest
        // scoring move.
        let mut best_score_so_far = 0;
        let mut best_index_so_far = 0;

        for (i, score) in self.move_scores.as_mut().unwrap().iter_mut().enumerate() {
            if *score > best_score_so_far {
                best_score_so_far = *score;
                best_index_so_far = i;
            }
        }

        if best_score_so_far == 0 {
            None
        } else {
            let mov = unsafe {
                self.move_list
                    .as_ref()
                    .unwrap()
                    .get_unchecked(best_index_so_far)
            };
            unsafe {
                *self
                    .move_scores
                    .as_mut()
                    .unwrap()
                    .get_unchecked_mut(best_index_so_far) = 0;
            };

            let phase = if best_score_so_far >= 100 {
                OrderingPhase::Captures(best_score_so_far)
            } else if best_score_so_far == 30 {
                OrderingPhase::Killers(true)
            } else if best_score_so_far == 20 {
                OrderingPhase::Killers(false)
            } else {
                self.quiet_moves_yielded += 1;
                OrderingPhase::Rest(self.quiet_moves_yielded)
            };

            Some((*mov, phase))
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OrderingPhase {
    /// The transposition table move.
    TTMove,
    /// Capturing moves. Contains the MVV-LVA score of the move.
    Captures(u16),
    /// Killer moves. Contains a bool which is true if this is the
    /// most recently stored killer move at this depth.
    Killers(bool),
    /// Everything else. Contains an ordinal count for which quiet move this is,
    /// starting at zero.
    Rest(u8),
}

impl fmt::Display for OrderingPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderingPhase::TTMove => write!(f, "TT Move"),
            OrderingPhase::Captures(score) => write!(f, "Capture (score: {})", score),
            OrderingPhase::Killers(newest) => {
                write!(f, "Killer: ({})", if *newest { "newest" } else { "oldest" })
            }
            OrderingPhase::Rest(count) => write!(f, "Rest: ({})", count),
        }
    }
}

impl<'a> Iterator for OrderedMoveList {
    type Item = (Move, OrderingPhase);
    fn next(&mut self) -> Option<Self::Item> {
        if !self.yielded_tt_move {
            self.yielded_tt_move = true;
            match self.tt_move {
                Some(mov) => {
                    return Some((mov, OrderingPhase::TTMove));
                }
                None => {}
            }
        }

        if !self.prepared {
            self.prepare_move_list();
        }

        self.yield_next()
    }
}
