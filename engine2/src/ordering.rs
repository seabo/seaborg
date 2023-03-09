//! Tools for ordering and iterating moves in a search environment.
use super::score::Score;
use super::search::Search;

use core::mov::Move;
use core::movelist::{BasicMoveList, MoveList};
use core::position::Position;

use num::FromPrimitive;
use num_derive::FromPrimitive;

use std::slice::Iter as SliceIter;

pub struct OrderedMoves {
    buf: BasicMoveList,
    cursor: usize,
    phase: Phase,
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
#[repr(u8)]
pub enum Phase {
    /// Before the first phase has been loaded.
    Pre,
    /// The move currently stored in the HashTable for this position, if any.
    HashTable,
    /// Promotions to a queen, if any.
    QueenPromotions,
    /// Captures which have static exchange evaluation (SEE) > 0; i.e. expected to win material.
    GoodCaptures,
    /// Captures which have SEE = 0; i.e. expected to be neutral material.
    EqualCaptures,
    /// Quiet moves appearing in the killer tables. Such a move caused a cutoff at the same ply in
    /// another variation, and is therefore considered likely to have a similarly positive effect
    /// in this position too.
    Killers,
    /// All other quiet (i.e. non-capturing or promoting) moves. These are further sorted according
    /// to the history heuristic, which scores moves based on how many times have they have caused
    /// cutoffs elsewhere in the tree.
    Quiet,
    /// Captures which have SEE < 0; i.e. expected to lose material.
    BadCaptures,
    /// Promotions to anything other than a queen. In almost every instance, promoting to something
    /// other than a queen is pointless.
    Underpromotions,
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

pub trait Loader {
    /// Load the hash move(s) into the passed `MoveList`.
    fn load_hash(&mut self, _movelist: &mut BasicMoveList) {}

    /// Load promotions into the passed `MoveList`.
    fn load_promotions(&mut self, _movelist: &mut BasicMoveList) {}

    /// Load captures into the passed `MoveList`.
    fn load_captures(&mut self, _movelist: &mut BasicMoveList) {}

    /// Load killers into the passed `MoveList`.
    fn load_killers(&mut self, _movelist: &mut BasicMoveList) {}

    /// Load quiet moves into the passed `MoveList`.
    fn load_quiets(&mut self, _movelist: &mut BasicMoveList) {}
}

impl OrderedMoves {
    pub fn new() -> Self {
        Self {
            buf: BasicMoveList::empty(),
            cursor: 0,
            phase: Phase::Pre,
        }
    }

    pub fn next_phase(&self) -> Phase {
        self.phase
    }

    pub fn load_next_phase<L: Loader>(&mut self, mut loader: L) -> bool {
        let res = self.phase.inc();
        if res {
            use Phase::*;
            match self.phase {
                Pre => {
                    unreachable!("since we have incremented, this can never happen");
                }
                HashTable => {
                    // No need to clear the buf here, because it is guaranteed to already be empty.
                    loader.load_hash(&mut self.buf);
                }
                QueenPromotions => {
                    self.buf.clear();
                }
                GoodCaptures => {
                    self.buf.clear();
                }
                EqualCaptures => {
                    self.buf.clear();
                }
                Killers => {
                    self.buf.clear();
                }
                Quiet => {
                    self.buf.clear();
                }
                BadCaptures => {
                    self.buf.clear();
                }
                Underpromotions => {
                    self.buf.clear();
                }
            }
        }

        res
    }
}

struct HashIter<'a>(SliceIter<'a, Move>);
struct QueenPromotionsIter;
struct GoodCapturesIter;
struct EqualCapturesIter;
struct KillersIter;
struct QuietIter;
struct BadCapturesIter;
struct UnderpromotionsIter;
enum IterInner<'a> {
    Empty(std::iter::Empty<&'a Move>),
    Hash(HashIter<'a>),
    // QueenPromotions(QueenPromotionsIter),
    // GoodCaptures(GoodCapturesIter),
    // EqualCaptures(EqualCapturesIter),
    // Killers(KillersIter),
    // Quiet(QuietIter),
    // BadCaptures(BadCapturesIter),
    // Underpromotions(UnderpromotionsIter),
}

pub struct Iter<'a>(IterInner<'a>);

impl<'a> HashIter<'a> {
    pub fn from(om: &'a mut OrderedMoves) -> Self {
        Self(om.buf.as_slice().into_iter())
    }
}

impl<'a> Iterator for HashIter<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a> Iterator for IterInner<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        use IterInner::*;
        match self {
            Empty(i) => i.next(),
            Hash(i) => i.next(),
            //QueenPromotions(i) => i.next(),
            //GoodCaptures(i) => i.next(),
            //EqualCaptures(i) => i.next(),
            //Killers(i) => i.next(),
            //Quiet(i) => i.next(),
            //BadCaptures(i) => i.next(),
            //Underpromotions(i) => i.next(),
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            Some(m) => Some(m),
            None => {
                // cleanup the `OrderedMoveList` (self.om)
                None
            }
        }
    }
}

impl<'a> IntoIterator for &'a mut OrderedMoves {
    type Item = &'a Move;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        use Phase::*;
        let iter = match self.phase {
            Pre => IterInner::Empty(Default::default()),
            HashTable => IterInner::Hash(HashIter::from(self)),
            QueenPromotions => IterInner::Empty(Default::default()),
            GoodCaptures => IterInner::Empty(Default::default()),
            EqualCaptures => IterInner::Empty(Default::default()),
            Killers => IterInner::Empty(Default::default()),
            Quiet => IterInner::Empty(Default::default()),
            BadCaptures => IterInner::Empty(Default::default()),
            Underpromotions => IterInner::Empty(Default::default()),
        };

        Iter(iter)
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
                self.count += self.pos.generate_moves::<BasicMoveList>().len();
            } else {
                let mut moves = OrderedMoves::<BasicMoveList>::new();
                // TODO
                // while moves.next_phase(&mut self.pos) {
                //     for mov in &mut moves {
                //         self.pos.make_move(&mov);
                //         self.perft_recurse(depth - 1);
                //         self.pos.unmake_move();
                //     }
                // }
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
