use core::mov::Move;
use core::movelist::MoveList;

/// A wrapper around a `MoveList`.
///
/// Implements `Iterator` and uses a 'selection sort' style algorithm
/// to return `Move`s in a priority ordering.
///
/// An `OrderedMoveList` consumes the underlying `MoveList`, so it won't
/// be available after the iteration.
pub struct OrderedMoveList {
    /// The underlying `MoveList`. This gets consumed by the `OrderedMoveList`
    /// and won't be available after the iteration.
    move_list: MoveList,
    /// A copy of the move currently in the transposition table for this position
    tt_move: Move,
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
    pub fn new(move_list: MoveList, tt_move: Option<Move>) -> Self {
        if let Some(tt_move) = tt_move {
            Self {
                move_list,
                tt_move,
                yielded: 0,
                yielded_tt_move: false,
                yielded_all_captures: false,
            }
        } else {
            Self {
                move_list,
                tt_move: Move::null(),
                yielded: 0,
                yielded_tt_move: true,
                yielded_all_captures: false,
            }
        }
    }
}

impl Iterator for OrderedMoveList {
    type Item = Move;
    fn next(&mut self) -> Option<Self::Item> {
        if self.yielded == self.move_list.len() {
            None
        } else {
            // 1. Do we need to yield the TT move?
            if !self.yielded_tt_move {
                self.yielded += 1;
                self.yielded_tt_move = true;
                return Some(self.tt_move);
            }
            // 2. Do we need to yield captures
            if !self.yielded_all_captures {
                // Yes - scan for the first capture
                for i in 0..self.move_list.len() {
                    let mov = unsafe { self.move_list.get_unchecked_mut(i) };
                    if mov.is_capture() {
                        self.yielded += 1;
                        let returned_move = mov.clone();
                        // set that entry to a null move and return it
                        *mov = Move::null();
                        return Some(returned_move);
                    }
                }
                // If we get here, then nothing was a capture
                self.yielded_all_captures = true;
            }
            // More blocks of moves according to some predicate (like `is_capture()`) would go here and follow the pattern of 2.
            // 3. Yield any remaining moves
            for i in 0..self.move_list.len() {
                let mov = unsafe { self.move_list.get_unchecked_mut(i) };
                if !mov.is_null() {
                    self.yielded += 1;
                    let returned_move = mov.clone();
                    // set that entry to a null move and return it
                    *mov = Move::null();
                    return Some(returned_move);
                }
            }
            // if we get all the way here, then we didn't find any moves at all, so return `None`
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::init::init_globals;
    use core::position::Position;
    #[test]
    fn orders_moves() {
        init_globals();

        let pos = Position::from_fen("4b3/4B1bq/p2Q2pp/4pp2/8/8/p7/k1K5 w - - 0 1").unwrap();
        let move_list = pos.generate_moves();
        let tt_move = move_list[4].clone();
        let mut ordered_move_list = OrderedMoveList::new(move_list, Some(tt_move));

        assert_eq!(ordered_move_list.next().unwrap(), tt_move);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), true);
        assert_eq!(ordered_move_list.next().unwrap().is_capture(), false);
    }
}
