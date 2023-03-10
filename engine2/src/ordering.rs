//! Tools for ordering and iterating moves in a search environment.
use super::score::Score;
use super::search::Search;

use core::mov::Move;
use core::movelist::{ArrayVec, BasicMoveList, MoveList, MAX_MOVES};
use core::position::{PieceType, Position};

use num::FromPrimitive;
use num_derive::FromPrimitive;

use std::mem::MaybeUninit;
use std::ops::Range;
use std::slice::Iter as SliceIter;
use std::slice::IterMut as SliceIterMut;

pub type ScoredMove = (Move, Score);

#[derive(Copy, Clone, Debug)]
struct Entry {
    sm: ScoredMove,
    yielded: bool,
}

/// An `ArrayVec` containing `ScoredMoves`.
#[derive(Debug)]
pub struct ScoredMoveList(ArrayVec<Entry, 254>);

/// An iterator over a `ScoredMoveList` which allows the `Move`s to be inspected and scores mutated.
pub struct Scorer<'a> {
    iter: <&'a mut [Entry] as IntoIterator>::IntoIter,
}

impl<'a> Iterator for Scorer<'a> {
    type Item = (&'a Move, &'a mut Score);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|entry| (&entry.sm.0, &mut entry.sm.1))
    }
}

impl<'a> From<&'a mut [Entry]> for Scorer<'a> {
    fn from(val: &'a mut [Entry]) -> Self {
        Self {
            iter: val.into_iter(),
        }
    }
}

impl MoveList for ScoredMoveList {
    fn empty() -> Self {
        ScoredMoveList(ArrayVec::new())
    }

    fn push(&mut self, mv: Move) {
        let entry = Entry {
            sm: (mv, Score::zero()),
            yielded: false,
        };

        self.0.push_val(entry);
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// A selection sort over a mutable `Entry` slice.
///
/// In move ordering, selection sort is expected to work best, because pre-sorting the entire list
/// is wasted effort in the many cases where we get an early cutoff. Additionally, for a small list
/// of elements O(n^2) algorithms can outperform if they have a low constant factor relative to
/// O(n*log n) algorithms with more constant overhead.
struct SelectionSort<'a> {
    segment: &'a mut [Entry],
    pred: fn(&ScoredMove) -> bool,
}

impl<'a> SelectionSort<'a> {
    /// Create a new selection sort iterator from a segment (`&mut [Entry]`) and a function
    /// representing a predicate on each `ScoredMove`. If the predicate returns `true` it is
    /// yielded, otherwise it is skipped (and its `yielded` flag remains unset).
    fn from(segment: &'a mut [Entry], pred: fn(&ScoredMove) -> bool) -> Self {
        Self { segment, pred }
    }
}

impl<'a> Iterator for SelectionSort<'a> {
    type Item = &'a Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Each time we get a call to next, we want to iterate through the entire list and find the
        // largest value of `Score` among the entries which have not been yielded.
        // Then we want to set the `yielded` flag on that entry to `true`, and return a reference
        // to the move. When we get through the whole list without seeing a `yielded = false`, we
        // return `None`.
        let mut max = Score::INF_N;
        let mut max_entry: MaybeUninit<&mut Entry> = MaybeUninit::uninit();
        let mut found_one: bool = false;

        for entry in &mut *self.segment {
            if !entry.yielded && entry.sm.1 > max && (self.pred)(&entry.sm) {
                max = entry.sm.1;
                max_entry.write(entry);
                found_one = true;
            }
        }

        if found_one {
            // SAFETY: we can assume this is initialized because `found_one` was true, which means
            // we wrote the entry into this.
            //
            // We can further transmute the lifetime to `'a` because we know we aren't modifying
            // the `Move` (only its `yielded` flag).
            unsafe {
                let max_entry = max_entry.assume_init();
                max_entry.yielded = true;
                Some(std::mem::transmute(max_entry))
            }
        } else {
            None
        }
    }
}
//
// struct PromoIter<'a> {
//     iter: SelectionSort<'a>,
// }
//
// impl<'a> PromoIter<'a> {
//     fn from(segment: &'a mut [Entry]) -> Self {
//         // First, score the captures higher than the non-captures. Note that we do not run SEE on
//         // promotion moves.
//         let scorer = Scorer::from(&mut *segment);
//
//         for (ref mov, ref mut score) in scorer {
//             if mov.is_capture() {
//                 *score = Score::cp(1);
//             }
//         }
//
//         Self {
//             iter: SelectionSort::from(segment, |_| true),
//         }
//     }
// }
//
// impl<'a> Iterator for PromoIter<'a> {
//     type Item = &'a Move;
//     fn next(&mut self) -> Option<Self::Item> {
//         self.iter.next()
//     }
// }
//
// struct GoodCaptIter<'a> {
//     iter: SelectionSort<'a>,
// }
//
// impl<'a> Iterator for GoodCaptIter<'a> {
//     type Item = &'a Move;
//     fn next(&mut self) -> Option<Self::Item> {
//         self.iter.next()
//     }
// }
//
// impl<'a> GoodCaptIter<'a> {
//     fn from(segment: &'a mut [Entry]) -> Self {
//         Self {
//             iter: SelectionSort::from(segment, |sm| sm.1 > Score::zero()),
//         }
//     }
// }
//
// struct EqualCaptIter<'a> {
//     iter: SelectionSort<'a>,
// }
//
// impl<'a> EqualCaptIter<'a> {
//     fn from(segment: &'a mut [Entry]) -> Self {
//         Self {
//             iter: SelectionSort::from(segment, |sm| sm.1 == Score::zero()),
//         }
//     }
// }
//
// struct BadCaptIter<'a> {
//     iter: SelectionSort<'a>,
// }
//
// impl<'a> BadCaptIter<'a> {
//     fn from(segment: &'a mut [Entry]) -> Self {
//         Self {
//             // It's technically fine to have a no-op for the predicate if these are coming
//             // last, since all the other captures will have been yielded already, and the only ones
//             // left with `yielded = false` are the bad captures. So we can save an op but be a bit
//             // less explicit and brittle / less resilient to future changes.
//             iter: SelectionSort::from(segment, |sm| sm.1 < Score::zero()),
//         }
//     }
// }

/// An iterator over the killer moves. These are not scored, but they must each be checked against
/// the hash table move to ensure that they haven't already been yielded, to avoid a re-search.
///
/// This iterator assumes that all moves in the killer segment are set to `yielded = false` - it
/// does not check this. (However, `yielded = false` may not reflect that the move has been yielded
/// during the hash move phase, so it _does_ check for this).
#[derive(Debug)]
struct KillerIter<'a> {
    killer_segment: SliceIterMut<'a, Entry>,
    hash_segment: &'a mut [Entry],
}

impl<'a> Iterator for KillerIter<'a> {
    type Item = &'a Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.killer_segment.next() {
            Some(entry) => {
                entry.yielded = true;

                // If this move matches one in the hash segment, skip it.
                for hm in self.hash_segment.iter() {
                    if entry.sm.0 == hm.sm.0 {
                        return self.next();
                    }
                }

                // Now we are safe to return it.
                Some(&entry.sm.0)
            }
            None => None,
        }
    }
}

/// An iterator over the quiet moves. These are scored and they must each be checked against
/// the hash table move to ensure that they haven't already been yielded, to avoid a re-search.
///
/// This iterator assumes that all moves in the quiet segment are set to `yielded = false` - it
/// does not check this. (However, `yielded = false` may not reflect that the move has been yielded
/// during the hash move phase, so it _does_ check for this).
struct QuietsIter<'a> {
    quiets_sel_sort: SelectionSort<'a>,
    hash_segment: &'a mut [Entry],
    killer_segment: &'a mut [Entry],
}

impl<'a> Iterator for QuietsIter<'a> {
    type Item = &'a Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.quiets_sel_sort.next() {
            Some(mov) => {
                // If this move matches one in the hash segment, skip it.
                for hm in self.hash_segment.iter() {
                    if *mov == hm.sm.0 {
                        return self.next();
                    }
                }

                // If this move matches one in the killer segment, skip it.
                for km in self.killer_segment.iter() {
                    if *mov == km.sm.0 {
                        return self.next();
                    }
                }

                // Now, we are safe to return it.
                Some(mov)
            }
            None => None,
        }
    }
}

/// A structure for managing move ordering during search.
///
/// Whenever movegen is required, create a new `OrderedMoves`. To use it, the caller must also have
/// a type which implements `Loader`. This allows `OrderedMoves` to do staged loading of moves, and
/// to receive scores for captures (usually, the `Loader` will run SEE on the captures) and scores
/// for quiet moves (usually, the `Loader` will consult a history table).
///
/// Moves are generated in phases, so each new phase requires a call to
/// `OrderedMoves:load_next_phase`, passing a `Loader`. If this returns `true`, then there are move
/// moves to be yielded. Whenever there are more moves to be generated, `&mut OrderedMoves` is
/// `IntoIterator` and will yield those moves. The moves will only be yielded _once_, so it is
/// probably confusing to produce this iterator from `&mut OrderedMoves`. (TODO: We should have a method
/// which returns it instead, to make clearer that it's a one time thing).
///
/// `OrderedMoves` is built on top of an `ArrayVec`, as this appears to be significantly more
/// performant than any solution involving overflows or allocations, since there is very little
/// overhead / pointer chasing / bounds checking to manage an `ArrayVec`. However, the downside is
/// that `OrderedMoves` is a large structure - currently 3KB. We will have one of these at every
/// ply, so if we are searching deeply we could reach 100KB of data on the stack, just for move
/// ordering structs.
///
/// TODO: one possibility to explore is if there is any merit in attaching a `MoveOrdering` system
/// to the `Position` itself "at the bottom of the stack", so to speak. We could make this have
/// enough space to store the moves for every ply in the search in a single `ArrayVec`. It would
/// get gnarly to implement...
pub struct OrderedMoves {
    buf: ScoredMoveList,
    /// The index of the start of the current segment. A new segment is created each time the
    /// `Phase` increments. So in practice, we'll have a segment for the `HashMove`, a segment for
    /// promotions, a segment for captures, a segment for killer moves, and a segment for quiet
    /// moves.
    segment_start: usize,
    hash_segment: Range<usize>,
    promo_segment: Range<usize>,
    capt_segment: Range<usize>,
    killer_segment: Range<usize>,
    quiet_segment: Range<usize>,
    underpromo_segment: Range<usize>,
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
    fn load_hash(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Load promotions into the passed `MoveList`.
    ///
    /// This function should only load queen promotions into the list. This saves time on move
    /// generation, because it is very unlikely that an underpromotion is the best move, and we will
    /// very likely have a cutoff before reaching those moves. If we do make it that, we can
    /// generate the underpromotions from the queen promotions.
    fn load_promotions(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Load captures into the passed `MoveList`.
    fn load_captures(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Provides an iterator over the capture moves, allowing the `Loader` to provide scores for
    /// each move.
    fn score_captures(&mut self, _scorer: Scorer) {}

    /// Load killers into the passed `MoveList`.
    fn load_killers(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Load quiet moves into the passed `MoveList`.
    fn load_quiets(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Provides an iterator over the quiet moves, allowing the `Loader` to provide scores for
    /// each move.
    fn score_quiets(&mut self, _scorer: Scorer) {}
}

impl OrderedMoves {
    pub fn new() -> Self {
        Self {
            buf: ScoredMoveList::empty(),
            segment_start: 0,
            hash_segment: Range::default(),
            promo_segment: Range::default(),
            capt_segment: Range::default(),
            killer_segment: Range::default(),
            quiet_segment: Range::default(),
            underpromo_segment: Range::default(),
            phase: Phase::Pre,
        }
    }

    pub fn next_phase(&self) -> Phase {
        self.phase
    }

    /// Record the location of the hash move segment in the underlying buffer, assuming that it
    /// starts at `self.segment_start` and ends at `self.buf.len()`. This method therefore assumes
    /// that it is being called immediately after the relevant moves have been loaded.
    fn set_hash_segment(&mut self) {
        self.hash_segment = Range {
            start: self.segment_start,
            end: self.buf.len(),
        };

        self.segment_start = self.buf.len();
    }

    /// Record the location of the promotion move segment in the underlying buffer, assuming that it
    /// starts at `self.segment_start` and ends at `self.buf.len()`. This method therefore assumes
    /// that it is being called immediately after the relevant moves have been loaded.
    fn set_promo_segment(&mut self) {
        self.promo_segment = Range {
            start: self.segment_start,
            end: self.buf.len(),
        };

        self.segment_start = self.buf.len();
    }

    /// Record the location of the capture move segment in the underlying buffer, assuming that it
    /// starts at `self.segment_start` and ends at `self.buf.len()`. This method therefore assumes
    /// that it is being called immediately after the relevant moves have been loaded.
    fn set_capt_segment(&mut self) {
        self.capt_segment = Range {
            start: self.segment_start,
            end: self.buf.len(),
        };

        self.segment_start = self.buf.len();
    }

    /// Record the location of the killer move segment in the underlying buffer, assuming that it
    /// starts at `self.segment_start` and ends at `self.buf.len()`. This method therefore assumes
    /// that it is being called immediately after the relevant moves have been loaded.
    fn set_killer_segment(&mut self) {
        self.killer_segment = Range {
            start: self.segment_start,
            end: self.buf.len(),
        };

        self.segment_start = self.buf.len();
    }

    /// Record the location of the quiet move segment in the underlying buffer, assuming that it
    /// starts at `self.segment_start` and ends at `self.buf.len()`. This method therefore assumes
    /// that it is being called immediately after the relevant moves have been loaded.
    fn set_quiet_segment(&mut self) {
        self.quiet_segment = Range {
            start: self.segment_start,
            end: self.buf.len(),
        };

        self.segment_start = self.buf.len();
    }

    /// Record the location of the underpromotion move segment in the underlying buffer, assuming that it
    /// starts at `self.segment_start` and ends at `self.buf.len()`. This method therefore assumes
    /// that it is being called immediately after the relevant moves have been loaded.
    fn set_underpromo_segment(&mut self) {
        self.underpromo_segment = Range {
            start: self.segment_start,
            end: self.buf.len(),
        };

        self.segment_start = self.buf.len();
    }

    /// Return the hash segment.
    #[inline]
    fn hash_segment(&self) -> &mut [Entry] {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.hash_segment.clone()) }
    }

    /// Return the promo segment.
    #[inline]
    fn promo_segment(&self) -> &mut [Entry] {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.promo_segment.clone()) }
    }

    /// Return the capture segment.
    #[inline]
    fn capt_segment(&self) -> &mut [Entry] {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.capt_segment.clone()) }
    }

    /// Return the killer segment.
    #[inline]
    fn killer_segment(&self) -> &mut [Entry] {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.killer_segment.clone()) }
    }

    /// Return the quiet segment.
    #[inline]
    fn quiets_segment(&self) -> &mut [Entry] {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.quiet_segment.clone()) }
    }

    /// Return the underpromo segment.
    #[inline]
    fn underpromo_segment(&self) -> &mut [Entry] {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.underpromo_segment.clone()) }
    }

    /// This is very unsafe. First, we do not bounds check the range. Even more unsafe is that we
    /// take an immutable reference to `self` and get a mutable reference into the underlying
    /// buffer from it. This is accomplished with a `std::mem::transmute`. This function must only
    /// be called when we know that nobody has an mutable reference over the `Move`s. Since our
    /// public interface never gives these out, we are fine.
    #[inline]
    unsafe fn segment_from_range(&self, rng: Range<usize>) -> &mut [Entry] {
        let ptr = self.buf.0.list_ptr() as *mut Entry;
        let start = ptr.add(rng.start);
        let end = ptr.add(rng.end);

        std::slice::from_mut_ptr_range(Range { start, end })
    }

    fn current_segment(&mut self) -> &mut [Entry] {
        unsafe {
            self.buf
                .0
                .get_slice_mut_unchecked(self.segment_start..self.buf.len())
        }
    }

    /// Load the next phase of moves into the buffer.
    ///
    /// This function calls methods on the passed in `loader` to fill the buffer with the moves
    /// need in the next phase.
    pub fn load_next_phase<L: Loader>(&mut self, mut loader: L) -> bool {
        let res = self.phase.inc();
        if res {
            use Phase::*;
            match self.phase {
                Pre => {
                    unreachable!("since we have incremented, this can never happen");
                }
                HashTable => {
                    loader.load_hash(&mut self.buf);
                    self.set_hash_segment();
                }
                QueenPromotions => {
                    loader.load_promotions(&mut self.buf);
                    self.set_promo_segment();
                }
                GoodCaptures => {
                    loader.load_captures(&mut self.buf);
                    self.set_capt_segment();

                    loader.score_captures(self.capt_segment().into());
                }
                EqualCaptures => { /* Nothing to do here */ }
                Killers => {
                    loader.load_killers(&mut self.buf);
                    self.set_killer_segment();
                }
                Quiet => {
                    loader.load_quiets(&mut self.buf);
                    self.set_quiet_segment();

                    loader.score_quiets(self.quiets_segment().into());
                }
                BadCaptures => { /* Nothing to do here */ }
                Underpromotions => { /* Nothing to do here */ }
            }
        }

        res
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    #[inline]
    fn hash_iter<'a>(&'a mut self) -> SelectionSort<'a> {
        SelectionSort::from(self.hash_segment(), |_| true)
    }

    fn promo_iter<'a>(&'a mut self) -> SelectionSort<'a> {
        let segment = self.promo_segment();

        // First, score the captures higher than the non-captures. Note that we do not run SEE on
        // promotion moves.
        let scorer = Scorer::from(&mut *segment);

        for (mov, score) in scorer {
            if mov.is_capture() {
                *score = Score::cp(1);
            }
        }

        SelectionSort::from(segment, |_| true)
    }

    #[inline]
    fn good_capt_iter<'a>(&'a mut self) -> SelectionSort<'a> {
        let segment = self.capt_segment();
        SelectionSort::from(segment, |sm| sm.1 > Score::zero())
    }

    #[inline]
    fn equal_capt_iter<'a>(&'a mut self) -> SelectionSort<'a> {
        let segment = self.capt_segment();
        SelectionSort::from(segment, |sm| sm.1 == Score::zero())
    }

    #[inline]
    fn killer_iter<'a>(&'a mut self) -> KillerIter<'a> {
        let killer_segment = self.killer_segment().iter_mut();
        let hash_segment = self.hash_segment();

        KillerIter {
            killer_segment,
            hash_segment,
        }
    }

    #[inline]
    fn quiets_iter<'a>(&'a mut self) -> QuietsIter<'a> {
        let hash_segment = self.hash_segment();
        let killer_segment = self.killer_segment();
        let quiets_segment = self.quiets_segment();

        QuietsIter {
            quiets_sel_sort: SelectionSort::from(quiets_segment, |_| true),
            hash_segment,
            killer_segment,
        }
    }

    #[inline]
    fn bad_capt_iter<'a>(&'a mut self) -> SelectionSort<'a> {
        let segment = self.capt_segment();

        // It's technically fine to have a no-op for the predicate if these are coming
        // last, since all the other captures will have been yielded already, and the only ones
        // left with `yielded = false` are the bad captures. So we can save an op but be a bit
        // less explicit and brittle / less resilient to future changes.
        SelectionSort::from(segment, |sm| sm.1 < Score::zero())
    }

    fn underpromo_iter<'a>(&'a mut self) -> SelectionSort<'a> {
        let mut ptr = unsafe { self.buf.0.over_bounds_ptr_mut() };

        #[inline]
        fn entry(mov: &Entry, pt: PieceType) -> Entry {
            Entry {
                sm: (mov.sm.0.set_promo_type(pt), Score::zero()),
                yielded: false,
            }
        }
        for e in self.promo_segment() {
            // SAFETY: it's safe to write to the end of the buffer
            unsafe {
                ptr.write(entry(e, PieceType::Rook));
                ptr = ptr.add(1);
                ptr.write(entry(e, PieceType::Knight));
                ptr = ptr.add(1);
                ptr.write(entry(e, PieceType::Bishop));
                ptr = ptr.add(1);
            }
        }

        self.set_underpromo_segment();
        let underpromo_segment = self.underpromo_segment();

        // Note that the captures will already be scored to come ahead of the non-captures, since
        // we copied the entries from the set of queen promotions, which got scored during their
        // phase.
        SelectionSort::from(underpromo_segment, |_| true)
    }
}

enum IterInner<'a> {
    Empty(std::iter::Empty<&'a Move>),
    Hash(SelectionSort<'a>),
    QueenPromotions(SelectionSort<'a>),
    GoodCaptures(SelectionSort<'a>),
    EqualCaptures(SelectionSort<'a>),
    Killers(KillerIter<'a>),
    Quiet(QuietsIter<'a>),
    BadCaptures(SelectionSort<'a>),
    Underpromotions(SelectionSort<'a>),
}

pub struct Iter<'a>(IterInner<'a>);

impl<'a> Iterator for IterInner<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        use IterInner::*;
        match self {
            Empty(i) => i.next(),
            Hash(i) => i.next(),
            QueenPromotions(i) => i.next(),
            GoodCaptures(i) => i.next(),
            EqualCaptures(i) => i.next(),
            Killers(i) => i.next(),
            Quiet(i) => i.next(),
            BadCaptures(i) => i.next(),
            Underpromotions(i) => i.next(),
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            Some(m) => Some(m),
            None => None,
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
            HashTable => IterInner::Hash(self.hash_iter()),
            QueenPromotions => IterInner::Empty(Default::default()),
            GoodCaptures => IterInner::Empty(Default::default()),
            EqualCaptures => IterInner::Empty(Default::default()),
            Killers => IterInner::Empty(Default::default()),
            Quiet => IterInner::Empty(Default::default()),
            BadCaptures => IterInner::Empty(Default::default()),
            Underpromotions => IterInner::Underpromotions(self.underpromo_iter()),
        };

        Iter(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perft::TESTS;
    use core::mono_traits::{All, Legal};

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
                let mut moves = OrderedMoves::new();
                while moves.load_next_phase(TestLoader::from(&mut self.pos)) {
                    for mov in &mut moves {
                        self.pos.make_move(&mov);
                        self.perft_recurse(depth - 1);
                        self.pos.unmake_move();
                    }
                }
            }
        }
    }

    struct TestLoader<'a> {
        pos: &'a mut Position,
    }

    impl<'a> TestLoader<'a> {
        fn from(pos: &'a mut Position) -> Self {
            Self { pos }
        }
    }

    impl<'a> Loader for TestLoader<'a> {
        fn load_hash(&mut self, movelist: &mut ScoredMoveList) {
            self.pos.generate_in_new::<_, All, Legal>(movelist);
        }
    }

    #[test]
    fn perft() {
        core::init::init_globals();

        for (p, d, r) in TESTS {
            let pos = Position::from_fen(p).unwrap();
            let perft = Perft::perft(pos, d);
            println!("{} -> {}", p, perft);
            assert_eq!(perft, r);
        }
    }
}
