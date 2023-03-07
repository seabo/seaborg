use crate::eval::Value;
use crate::search::search::TTData;
use crate::tables::Table;

use core::mov::Move;
use core::movelist::{BasicMoveList, MAX_MOVES};
use core::position::Position;

use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

/// A wrapper around a `MoveList`.
///
/// Implements `Iterator` and uses a 'selection sort' style algorithm
/// to return `Move`s in a priority ordering.
///
/// An `OrderedMoveList` consumes the underlying `MoveList`, so it won't
/// be available after the iteration.
pub struct OrderedMoveList {
    /// A reference to the transposition table associated with this `OrderedMoveList`.
    tt: Rc<RefCell<Table<TTData>>>,
    /// The underlying `MoveList`. This gets consumed by the `OrderedMoveList`
    /// and won't be available after the iteration.
    pub move_list: Option<BasicMoveList>,
    /// Parallel array to the `move_list`, containing ordering scores of the associated moves.
    move_scores: Option<[i32; MAX_MOVES]>,
    /// The transposition table move.
    tt_move: Option<Move>,
    /// Tracks whether we have yielded the transposition table move yet
    yielded_tt_move: bool,
}

impl OrderedMoveList {
    pub fn new(pos: &Position, tt: Rc<RefCell<Table<TTData>>>) -> OrderedMoveList {
        let mut list = Self {
            tt,
            move_list: None,
            move_scores: None,
            tt_move: None,
            yielded_tt_move: false,
        };

        let tt_move = list.get_tt_move(pos);
        list.tt_move = tt_move;
        list.prepare_move_list(pos);

        list
    }

    fn tt(&self) -> Ref<'_, Table<TTData>> {
        self.tt.borrow()
    }

    fn get_tt_move(&self, pos: &Position) -> Option<Move> {
        match self.tt().get(pos) {
            Some(tt_entry) => Some(tt_entry.best_move()),
            None => None,
        }
    }

    fn score_move(&self, mov: Move, pos: &Position) -> i32 {
        if mov.is_null() {
            0
        } else if mov.is_capture() {
            let victim_value = pos.piece_at_sq(mov.dest()).type_of().value();
            let attacker_value = pos.piece_at_sq(mov.orig()).type_of().value();
            10000 + victim_value - attacker_value
        } else {
            10
        }
    }

    fn prepare_move_list(&mut self, pos: &Position) {
        let moves = pos.generate_moves();
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
            let score = self.score_move(*mov, pos);
            unsafe {
                *move_scores.get_unchecked_mut(i) = score;
            }
        }

        self.move_scores = Some(move_scores);

        // TODO: once killer moves are implemented, remove them from the list
        // killer moves as we'll already be returning them before getting this far.
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

            let phase = if mov.is_capture() {
                OrderingPhase::Captures
            } else {
                OrderingPhase::Rest
            };

            Some((*mov, phase))
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OrderingPhase {
    TTMove,
    Captures,
    Rest,
}

impl fmt::Display for OrderingPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderingPhase::TTMove => write!(f, "TT Move"),
            OrderingPhase::Captures => write!(f, "Capture"),
            OrderingPhase::Rest => write!(f, "Rest"),
        }
    }
}

impl Iterator for OrderedMoveList {
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

        self.yield_next()
    }
}
