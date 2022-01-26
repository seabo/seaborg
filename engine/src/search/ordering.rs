use crate::search::search::{Search, TTData};
use crate::tables::Table;

use core::mov::Move;
use core::movelist::MoveList;
use core::position::Position;

use log::info;

use std::cell::{Ref, RefCell};
use std::rc::Rc;

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
    /// Tracks how many `Move`s have so far been yielded by the iteration.
    /// When this reaches `MoveList.len` then we can halt the iteration by
    /// returning `None`.
    yielded: usize,
    /// Tracks whether we have yielded the transposition table move yet
    yielded_tt_move: bool,
    /// Tracks whether we have yielded every capture yet
    yielded_all_captures: bool,
}

impl OrderedMoveList {
    pub fn new(pos: Rc<RefCell<Position>>, tt: Rc<RefCell<Table<TTData>>>) -> Self {
        Self {
            pos,
            tt,
            move_list: None,
            yielded: 0,
            yielded_tt_move: false,
            yielded_all_captures: false,
        }
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
}

impl<'a> Iterator for OrderedMoveList {
    type Item = Move;
    fn next(&mut self) -> Option<Self::Item> {
        if !self.yielded_tt_move {
            // 1. Set the yielded flag to true, even if we aren't going to yield anything
            self.yielded_tt_move = true;
            // 1. Yield the tt move, if any
            match self.get_tt_move() {
                Some(mov) => {
                    self.yielded += 1;
                    return Some(mov);
                }
                None => {}
            }
        }

        if let None = self.move_list {
            let moves = self.pos().generate_moves();
            self.move_list = Some(moves);
        }

        let move_list = self.move_list.as_deref_mut().unwrap();

        if !self.yielded_all_captures {
            // The unwrap should be safe because we have just set `self.move_list` to
            // `Some`.
            for i in 0..move_list.len() {
                let mov = unsafe { move_list.get_unchecked_mut(i) };
                if mov.is_capture() {
                    self.yielded += 1;
                    let returned_move = mov.clone();
                    *mov = Move::null();
                    return Some(returned_move);
                }
            }
            // If we get here, then nothing was a capture.
            self.yielded_all_captures = true;
        }

        for i in 0..move_list.len() {
            let mov = unsafe { move_list.get_unchecked_mut(i) };
            if !mov.is_null() {
                self.yielded += 1;
                let returned_move = mov.clone();
                *mov = Move::null();
                return Some(returned_move);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::init::init_globals;
    use core::position::Position;
    // #[test]
    // fn orders_moves() {
    //     init_globals();

    //     let pos = Position::from_fen("4b3/4B1bq/p2Q2pp/4pp2/8/8/p7/k1K5 w - - 0 1").unwrap();
    //     let move_list = pos.generate_moves();
    //     let tt_move = move_list[4].clone();
    //     let mut ordered_move_list = OrderedMoveList::new(move_list, Some(tt_move));

    //     assert_eq!(ordered_move_list.next().unwrap(), tt_move);
    //     assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
    //     assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
    //     assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
    //     assert_eq!(ordered_move_list.next().unwrap().is_capture(), false);
    // }
}
