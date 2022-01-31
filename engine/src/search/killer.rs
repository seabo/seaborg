use core::mov::Move;
use core::position::Position;
use core::position::Square;

use std::cell::RefCell;
use std::rc::Rc;

/// The maximum depth for which we store killer moves. Determines the size of the array
/// kept on the `Search` struct.
const MAX_KILLER_DEPTH: usize = 50;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct KillerMove {
    orig: Square,
    dest: Square,
    is_castle: bool,
}

impl KillerMove {
    pub fn new(mov: Move, is_castle: bool) -> Self {
        KillerMove {
            orig: mov.orig(),
            dest: mov.dest(),
            is_castle,
        }
    }
}

impl From<Move> for KillerMove {
    fn from(mov: Move) -> Self {
        Self {
            orig: mov.orig(),
            dest: mov.dest(),
            is_castle: mov.is_castle(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct KillerArray {
    pos: Rc<RefCell<Position>>,
    /// Array used to store 2 killer moves for each ply distance from the root node.
    arr: [(Option<KillerMove>, Option<KillerMove>); MAX_KILLER_DEPTH],
}

impl KillerArray {
    pub fn new(pos: Rc<RefCell<Position>>) -> Self {
        KillerArray {
            arr: [(None, None); MAX_KILLER_DEPTH],
            pos,
        }
    }

    /// Add a `Move` to the killer moves array. This will evict an existing killer move at
    /// this depth if both slots are already taken. In such a case, the oldest killer move
    /// is evicted first.
    pub fn add_killer(&mut self, mov: Move, tt_move: Option<Move>, depth_from_root: u8) {
        // Only add if the mov satisfies requirements: not a capture, not the TT move
        if mov.is_capture() || Some(mov) == tt_move {
            return;
        }

        // Get the current killer moves for this depth.
        let current_entry = self.get_killers(depth_from_root);

        // Convert the `Move` we are planning to save into a `KillerMove`.
        let killer_move = KillerMove::from(mov);
        // Check that the new killer is not duplicative of one we already have.
        if current_entry.0 == Some(killer_move) || current_entry.1 == Some(killer_move) {
            return;
        }

        let mut new_entry: (Option<KillerMove>, Option<KillerMove>) = current_entry.clone();

        // If both slots already filled, move the second to the first slot, evict the second
        if let (Some(_), Some(mov_1)) = new_entry {
            new_entry.0 = Some(mov_1); // Shift over the previous second entry.
            new_entry.1 = Some(killer_move); // Add in the new killer move.
        } else if let None = new_entry.0 {
            // Fill an empty slot.
            new_entry.0 = Some(killer_move);
        } else if let None = new_entry.1 {
            // Fill an empty slot.
            new_entry.1 = Some(killer_move);
        }

        self.set_killers(depth_from_root, new_entry);
    }

    /// Get the `KillerMoves` for a given depth from the root note.
    fn get_killers(&self, depth_from_root: u8) -> (Option<KillerMove>, Option<KillerMove>) {
        match self.get_entry(depth_from_root) {
            Some(entry) => *entry,
            None => (None, None),
        }
    }
    /// Get the killer moves as `Move` structs for the given depth from the root node.
    pub fn get_killers_as_moves(&self, depth_from_root: u8) -> (Option<Move>, Option<Move>) {
        let (killer_1, killer_2) = self.get_killers(depth_from_root);
        let move_1 = self.map_killer_to_move(killer_1);
        let move_2 = self.map_killer_to_move(killer_2);

        (move_1, move_2)
    }

    fn map_killer_to_move(&self, killer: Option<KillerMove>) -> Option<Move> {
        match killer {
            Some(k) => self
                .pos
                .borrow()
                .is_non_capturing_move(k.orig, k.dest, k.is_castle),
            None => None,
        }
    }

    fn get_entry(&self, depth_from_root: u8) -> Option<&(Option<KillerMove>, Option<KillerMove>)> {
        if depth_from_root > MAX_KILLER_DEPTH as u8 {
            // We are too deep in the search tree, so killer moves are not being used.
            // Return nothing.
            return None;
        }
        // Safety: we checked above that `depth_from_root` does not exceed the bounds of the
        // array, so this is safe.
        let entry = unsafe { self.arr.get_unchecked(depth_from_root as usize) };
        Some(entry)
    }

    fn get_entry_mut(
        &mut self,
        depth_from_root: u8,
    ) -> Option<&mut (Option<KillerMove>, Option<KillerMove>)> {
        if depth_from_root > MAX_KILLER_DEPTH as u8 {
            // We are too deep in the search tree, so killer moves are not being used.
            // Return nothing.
            return None;
        }
        // Safety: we checked above that `depth_from_root` does not exceed the bounds of the
        // array, so this is safe.
        let entry = unsafe { self.arr.get_unchecked_mut(depth_from_root as usize) };
        Some(entry)
    }

    fn set_killers(
        &mut self,
        depth_from_root: u8,
        new_entry: (Option<KillerMove>, Option<KillerMove>),
    ) {
        match self.get_entry_mut(depth_from_root) {
            Some(entry) => {
                *entry = new_entry;
            }
            None => {}
        };
    }
}
