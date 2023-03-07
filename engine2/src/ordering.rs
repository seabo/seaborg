//! Tools for ordering and iterating moves in a search environment.
use super::search::Search;

use core::mov::Move;
use core::movelist::MoveList;
use core::position::Position;

use num::FromPrimitive;
use num_derive::FromPrimitive;

pub struct OrderedMoves {
    moves: MoveList,
    phase: Phase,
    /// Cursor indicating current location in the move list iteration.
    cursor: *mut Move,
    /// Raw pointer to the final move in the list. When `cursor == end` we can return `None` from
    /// `next`.
    end: *mut Move,
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
#[repr(u8)]
pub enum Phase {
    /// The move currently stored in the HashTable for this position, if any.
    HashTable = 0,
    /// Promotions to a queen, if any.
    QueenPromotions = 1,
    /// Captures which have static exchange evaluation (SEE) > 0; i.e. expected to win material.
    GoodCaptures = 2,
    /// Captures which have SEE = 0; i.e. expected to be neutral material.
    EqualCaptures = 3,
    /// Quiet moves appearing in the killer tables. Such a move caused a cutoff at the same ply in
    /// another variation, and is therefore considered likely to have a similarly positive effect
    /// in this position too.
    Killers = 4,
    /// All other quiet (i.e. non-capturing or promoting) moves. These are further sorted according
    /// to the history heuristic, which scores moves based on how many times have they have caused
    /// cutoffs elsewhere in the tree.
    Quiet = 5,
    /// Captures which have SEE < 0; i.e. expected to lose material.
    BadCaptures = 6,
    /// Promotions to anything other than a queen. In almost every instance, promoting to something
    /// other than a queen is pointless.
    Underpromotions = 7,
}

impl Phase {
    pub fn inc(&mut self) -> bool {
        match FromPrimitive::from_u8(*self as u8 + 1) {
            Some(p) => {
                *self = p;
                true
            }
            None => false,
        }
    }
}

impl OrderedMoves {
    pub fn new() -> Self {
        Self {
            moves: Default::default(),
            phase: Phase::HashTable,
            cursor: std::ptr::null_mut(),
            end: std::ptr::null_mut(),
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn next_phase(&mut self, pos: &mut Position /* search: &mut Search */) -> bool {
        use Phase::*;
        match self.phase {
            HashTable => {
                self.load_hash_phase(pos);
            }
            QueenPromotions => {}
            GoodCaptures => {}
            EqualCaptures => {}
            Killers => {}
            Quiet => {}
            BadCaptures => {}
            Underpromotions => {}
        }

        self.phase.inc()
    }

    /// Assumes that the hash phase has not yet been loaded. Undefined behaviour can arise if
    /// called when this invariant doesn't hold. Private function used in internal implementation
    /// only.
    fn load_hash_phase(&mut self, pos: &Position) {
        // For now, we load all moves at hash phase time
        self.moves = pos.generate_moves();
        if self.moves.len() > 0 {
            self.cursor = &mut self.moves[0] as *mut Move;
        }
        self.end = unsafe { self.cursor.offset(self.moves.len() as isize) };
    }
}

impl Iterator for &mut OrderedMoves {
    type Item = Move;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == self.end {
            None
        } else {
            let mov = unsafe { *self.cursor };
            unsafe { self.cursor = self.cursor.offset(1) };
            Some(mov)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perft::TESTS;

    struct Perft {
        pos: Position,
        count: usize,
    }

    impl Perft {
        pub fn perft(pos: Position, depth: usize) -> usize {
            let mut p = Perft { pos, count: 0 };

            p.perft_recurse(depth);
            p.count
        }

        fn perft_recurse(&mut self, depth: usize) {
            if depth == 1 {
                self.count += self.pos.generate_moves().len();
            } else {
                let mut moves = OrderedMoves::new();
                while moves.next_phase(&mut self.pos) {
                    for mov in &mut moves {
                        self.pos.make_move(mov);
                        self.perft_recurse(depth - 1);
                        self.pos.unmake_move();
                    }
                }
            }
        }
    }

    #[test]
    fn perft() {
        core::init::init_globals();

        for (p, d, r) in TESTS {
            let pos = Position::from_fen(p).unwrap();
            assert_eq!(Perft::perft(pos, d), r);
        }
    }
}
