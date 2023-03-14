//! Tools for ordering and iterating moves in a search environment.
use super::score::Score;

use core::mov::Move;
use core::movelist::{ArrayVec, MoveList};
use core::position::PieceType;

use num::FromPrimitive;
use num_derive::FromPrimitive;

use std::cell::UnsafeCell;
use std::iter::Chain;
use std::mem::MaybeUninit;
use std::ops::Range;
use std::slice::Iter as SliceIter;

pub type ScoredMove = (Move, Score);

/// An entry in the move ordering `ArrayVec` buffer.
#[derive(Debug)]
struct Entry {
    sm: ScoredMove,
    yielded: UnsafeCell<bool>,
}

/// An `ArrayVec` containing `ScoredMoves`.
#[derive(Debug)]
pub struct ScoredMoveList(ArrayVec<Entry, 254>);

/// A slice of `Entry`'s.
type Segment<'a> = &'a [Entry];

/// A mutable slice of `Entry`'s.
type SegmentMut<'a> = &'a mut [Entry];

/// An iterator over a `ScoredMoveList` which allows the `Move`s to be inspected and scores mutated.
pub struct Scorer<'a> {
    iter: <SegmentMut<'a> as IntoIterator>::IntoIter,
}

impl<'a> Iterator for Scorer<'a> {
    type Item = (&'a Move, &'a mut Score);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|entry| (&entry.sm.0, &mut entry.sm.1))
    }
}

impl<'a> From<SegmentMut<'a>> for Scorer<'a> {
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
            yielded: UnsafeCell::new(false),
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

/// A selection sort over a `Segment`.
///
/// In move ordering, selection sort is expected to work best, because pre-sorting the entire list
/// is wasted effort in the many cases where we get an early cutoff. Additionally, for a small list
/// of elements O(n^2) algorithms can outperform if they have a low constant factor relative to
/// O(n*log n) algorithms with more constant overhead.
struct SelectionSort<'a> {
    segment: Segment<'a>,
    pred: fn(&ScoredMove) -> bool,
}

impl<'a> SelectionSort<'a> {
    /// Create a new selection sort iterator from a `Segment` and a function
    /// representing a predicate on each `ScoredMove`. If the predicate returns `true` it is
    /// yielded, otherwise it is skipped (and its `yielded` flag remains unset).
    fn from(segment: Segment<'a>, pred: fn(&ScoredMove) -> bool) -> Self {
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
        let mut max_entry: MaybeUninit<&Entry> = MaybeUninit::uninit();
        let mut found_one: bool = false;

        for entry in self.segment {
            // SAFETY: we can read the private `yielded` flag here.
            let yielded = unsafe { *entry.yielded.get() };
            if !yielded && entry.sm.1 > max && (self.pred)(&entry.sm) {
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
                *max_entry.yielded.get() = true;
                Some(&max_entry.sm.0)
            }
        } else {
            None
        }
    }
}

type PromotionsIter<'a> = Chain<SelectionSort<'a>, SelectionSort<'a>>;

/// An iterator over the killer moves. These are not scored, but they must each be checked against
/// the hash table move to ensure that they haven't already been yielded, to avoid a re-search.
///
/// This iterator assumes that all moves in the killer segment are set to `yielded = false` - it
/// does not check this. (However, `yielded = false` may not reflect that the move has been yielded
/// during the hash move phase, so it _does_ check for this).
#[derive(Debug)]
struct KillerIter<'a> {
    killer_segment: SliceIter<'a, Entry>,
    hash_segment: Segment<'a>,
}

impl<'a> Iterator for KillerIter<'a> {
    type Item = &'a Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.killer_segment.next() {
            Some(entry) => {
                // SAFETY: we can set the private `yielded` flag. Nobody on the outside can have a
                // reference to it.
                unsafe {
                    *entry.yielded.get() = true;
                }

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
    hash_segment: Segment<'a>,
    killer_segment: Segment<'a>,
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
/// probably confusing to produce this iterator from `&mut OrderedMoves`.
///
/// `OrderedMoves` is built on top of an `ArrayVec`, as this appears to be significantly more
/// performant than any solution involving overflows or allocations, since there is very little
/// overhead / pointer chasing / bounds checking to manage an `ArrayVec`. However, the downside is
/// that `OrderedMoves` is a large structure - currently 3KB. We will have one of these at every
/// ply, so if we are searching deeply we could reach 100KB of data on the stack, just for move
/// ordering structs.
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
    fn hash_segment(&self) -> Segment<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.hash_segment.clone()) }
    }

    /// Return the promo segment.
    #[inline]
    fn promo_segment(&self) -> Segment<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.promo_segment.clone()) }
    }

    /// Return the capture segment.
    #[inline]
    fn capt_segment(&self) -> Segment<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.capt_segment.clone()) }
    }

    /// Return the killer segment.
    #[inline]
    fn killer_segment(&self) -> Segment<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.killer_segment.clone()) }
    }

    /// Return the quiet segment.
    #[inline]
    fn quiets_segment(&self) -> Segment<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.quiet_segment.clone()) }
    }

    /// Return the underpromo segment.
    #[inline]
    fn underpromo_segment(&self) -> Segment<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range(self.underpromo_segment.clone()) }
    }

    /// Return the hash segment.
    #[inline]
    fn hash_segment_mut(&mut self) -> SegmentMut<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range_mut(self.hash_segment.clone()) }
    }

    /// Return the promo segment.
    #[inline]
    fn promo_segment_mut(&mut self) -> SegmentMut<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range_mut(self.promo_segment.clone()) }
    }

    /// Return the capture segment.
    #[inline]
    fn capt_segment_mut(&mut self) -> SegmentMut<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range_mut(self.capt_segment.clone()) }
    }

    /// Return the killer segment.
    #[inline]
    fn killer_segment_mut(&mut self) -> SegmentMut<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range_mut(self.killer_segment.clone()) }
    }

    /// Return the quiet segment.
    #[inline]
    fn quiets_segment_mut(&mut self) -> SegmentMut<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range_mut(self.quiet_segment.clone()) }
    }

    /// Return the underpromo segment.
    #[inline]
    fn underpromo_segment_mut(&mut self) -> SegmentMut<'_> {
        // SAFETY: the segment `Range` starts as `0..0`, which is always fine for us to get.
        // We only ever change the `Range` when we know that moves have been placed in that
        // location, so we are safe to derefence.
        unsafe { self.segment_from_range_mut(self.underpromo_segment.clone()) }
    }

    /// This is very unsafe. First, we do not bounds check the range. Even more unsafe is that we
    /// take an immutable reference to `self` and get a mutable reference into the underlying
    /// buffer from it. This is accomplished with a `std::mem::transmute`. This function must only
    /// be called when we know that nobody has an mutable reference over the `Move`s. Since our
    /// public interface never gives these out, we are fine.
    #[inline]
    unsafe fn segment_from_range(&self, rng: Range<usize>) -> Segment<'_> {
        let ptr = self.buf.0.list_ptr();
        let start = ptr.add(rng.start);
        let end = ptr.add(rng.end);

        std::slice::from_ptr_range(Range { start, end })
    }

    /// This is very unsafe. First, we do not bounds check the range. Even more unsafe is that we
    /// take an immutable reference to `self` and get a mutable reference into the underlying
    /// buffer from it. This is accomplished with a `std::mem::transmute`. This function must only
    /// be called when we know that nobody has an mutable reference over the `Move`s. Since our
    /// public interface never gives these out, we are fine.
    #[inline]
    unsafe fn segment_from_range_mut(&mut self, rng: Range<usize>) -> SegmentMut<'_> {
        let ptr = self.buf.0.list_ptr_mut();
        let start = ptr.add(rng.start);
        let end = ptr.add(rng.end);

        std::slice::from_mut_ptr_range(Range { start, end })
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
                    self.dedup_segments(self.promo_segment(), self.hash_segment());
                }
                GoodCaptures => {
                    loader.load_captures(&mut self.buf);
                    self.set_capt_segment();

                    loader.score_captures(self.capt_segment_mut().into());
                    self.dedup_segments(self.capt_segment(), self.hash_segment());
                }
                EqualCaptures => { /* Nothing to do here */ }
                Killers => {
                    loader.load_killers(&mut self.buf);
                    self.set_killer_segment();
                    self.dedup_segments(self.killer_segment(), self.hash_segment());
                }
                Quiet => {
                    loader.load_quiets(&mut self.buf);
                    self.set_quiet_segment();

                    loader.score_quiets(self.quiets_segment_mut().into());
                    self.dedup_segments(self.quiets_segment(), self.hash_segment());
                }
                BadCaptures => { /* Nothing to do here */ }
                Underpromotions => {
                    self.prepare_underpromotions();
                    self.dedup_segments(self.underpromo_segment(), self.hash_segment());
                }
            }
        }

        res
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Iterates through the `target` segment, marking any moves which match moves in the i
    /// `src` segment as already yielded.
    fn dedup_segments(&self, tgt: Segment<'_>, src: Segment<'_>) {
        for tgt_entry in tgt {
            for src_entry in src {
                if tgt_entry.sm.0 == src_entry.sm.0 {
                    // SAFETY: we can mark entries as yielded, since no-one outside the API has a
                    // reference to the `yielded` flag.
                    unsafe {
                        *tgt_entry.yielded.get() = true;
                    }
                }
            }
        }
    }

    fn prepare_underpromotions(&mut self) {
        let mut ptr = unsafe { self.buf.0.over_bounds_ptr() as *mut Entry };

        #[inline]
        fn entry(mov: &Entry, pt: PieceType) -> Entry {
            Entry {
                sm: (mov.sm.0.set_promo_type(pt), Score::zero()),
                yielded: UnsafeCell::new(false),
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

        // SAFETY: we need to update the buffer's length to maintain the invariant.
        unsafe {
            let added: usize = self.promo_segment().len() * 3;
            self.buf.0.set_len(self.buf.0.len() + added);
        }

        self.set_underpromo_segment();
    }

    #[inline]
    fn hash_iter<'a>(&'a self) -> SelectionSort<'a> {
        SelectionSort::from(self.hash_segment(), |_| true)
    }

    #[inline]
    fn promo_iter<'a>(&'a self) -> PromotionsIter<'a> {
        let segment = self.promo_segment();
        SelectionSort::from(segment, |m| m.0.is_capture())
            .chain(SelectionSort::from(segment, |_| true))
    }

    #[inline]
    fn good_capt_iter<'a>(&'a self) -> SelectionSort<'a> {
        let segment = self.capt_segment();
        SelectionSort::from(segment, |sm| sm.1 > Score::zero())
    }

    #[inline]
    fn equal_capt_iter<'a>(&'a self) -> SelectionSort<'a> {
        let segment = self.capt_segment();
        SelectionSort::from(segment, |sm| sm.1 == Score::zero())
    }

    #[inline]
    fn killer_iter<'a>(&'a self) -> KillerIter<'a> {
        let killer_segment = self.killer_segment().iter();
        let hash_segment = self.hash_segment();

        KillerIter {
            killer_segment,
            hash_segment,
        }
    }

    #[inline]
    fn quiets_iter<'a>(&'a self) -> QuietsIter<'a> {
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
    fn bad_capt_iter<'a>(&'a self) -> SelectionSort<'a> {
        let segment = self.capt_segment();

        // It's technically fine to have a no-op for the predicate if these are coming
        // last, since all the other captures will have been yielded already, and the only ones
        // left with `yielded = false` are the bad captures. So we can save an op but be a bit
        // less explicit and brittle / less resilient to future changes.
        SelectionSort::from(segment, |sm| sm.1 < Score::zero())
    }

    fn underpromo_iter<'a>(&'a self) -> PromotionsIter<'a> {
        let segment = self.underpromo_segment();

        SelectionSort::from(segment, |m| m.0.is_capture())
            .chain(SelectionSort::from(segment, |_| true))
    }
}

enum IterInner<'a> {
    Empty(std::iter::Empty<&'a Move>),
    Hash(SelectionSort<'a>),
    QueenPromotions(PromotionsIter<'a>),
    GoodCaptures(SelectionSort<'a>),
    EqualCaptures(SelectionSort<'a>),
    Killers(KillerIter<'a>),
    Quiet(QuietsIter<'a>),
    BadCaptures(SelectionSort<'a>),
    Underpromotions(PromotionsIter<'a>),
}

pub struct PhaseIter<'a>(IterInner<'a>);

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

impl<'a> Iterator for PhaseIter<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            Some(m) => Some(m),
            None => None,
        }
    }
}

impl<'a> IntoIterator for &'a OrderedMoves {
    type Item = &'a Move;
    type IntoIter = PhaseIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        use Phase::*;
        let iter = match self.phase {
            Pre => IterInner::Empty(Default::default()),
            HashTable => IterInner::Hash(self.hash_iter()),
            QueenPromotions => IterInner::QueenPromotions(self.promo_iter()),
            GoodCaptures => IterInner::GoodCaptures(self.good_capt_iter()),
            EqualCaptures => IterInner::EqualCaptures(self.equal_capt_iter()),
            Killers => IterInner::Killers(self.killer_iter()),
            Quiet => IterInner::Quiet(self.quiets_iter()),
            BadCaptures => IterInner::BadCaptures(self.bad_capt_iter()),
            Underpromotions => IterInner::Underpromotions(self.underpromo_iter()),
        };

        PhaseIter(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perft::TESTS;
    use core::mono_traits::{All, Captures, Legal, QueenPromotions, Quiets};
    use core::movelist::BasicMoveList;
    use core::position::Position;

    use rand::*;

    struct Perft {
        pos: Position,
        count: usize,
    }

    impl Perft {
        pub fn perft(pos: Position, depth: usize) -> usize {
            let mut p = Perft { pos, count: 0 };

            // Divide perft output for debugging.
            let mut moves = OrderedMoves::new();
            let mut c: usize = 0;
            while moves.load_next_phase(TestLoader::from(&mut p.pos)) {
                println!("{:?}", moves.phase());
                for mov in &moves {
                    c += 1;
                    p.pos.make_move(&mov);
                    let child_count = if depth == 1 {
                        1
                    } else {
                        p.perft_recurse(depth - 1)
                    };
                    println!("{}: {}", mov, child_count);
                    p.pos.unmake_move();
                }
            }
            println!("{} moves in the passed position", c);

            // p.perft_recurse(depth);
            p.count
        }

        fn perft_recurse(&mut self, depth: usize) -> usize {
            if depth == 1 {
                let c = self.pos.generate::<BasicMoveList, All, Legal>().len();
                self.count += c;
                c
            } else {
                let mut moves = OrderedMoves::new();
                let mut c: usize = 0;
                while moves.load_next_phase(TestLoader::from(&mut self.pos)) {
                    for mov in &moves {
                        self.pos.make_move(&mov);
                        c += self.perft_recurse(depth - 1);
                        self.pos.unmake_move();
                    }
                }
                c
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
            match self.pos.generate::<BasicMoveList, All, Legal>().random() {
                Some(mv) => {
                    movelist.push(*mv);
                }
                None => {}
            }
        }

        fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
            self.pos.generate_in::<_, QueenPromotions, Legal>(movelist);
        }

        fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
            self.pos.generate_in::<_, Captures, Legal>(movelist);
        }

        fn load_killers(&mut self, movelist: &mut ScoredMoveList) {
            // Insert two random moves into the killer segment.
            let all_moves = self.pos.generate::<BasicMoveList, Quiets, Legal>();

            if all_moves.len() == 1 {
                movelist.push(*all_moves.first().unwrap());
            } else if all_moves.len() > 1 {
                let first = *all_moves.random().unwrap();
                let second;

                loop {
                    let tmp = *all_moves.random().unwrap();
                    if tmp != first {
                        second = tmp;
                        break;
                    }
                }

                movelist.push(first);
                movelist.push(second);
            }
        }

        fn load_quiets(&mut self, movelist: &mut ScoredMoveList) {
            self.pos.generate_in::<_, Quiets, Legal>(movelist);
        }

        fn score_captures(&mut self, captures: Scorer) {
            let mut rng = rand::thread_rng();

            for (mov, score) in captures {
                if mov.is_capture() {
                    // Assign a random number from -10_000 to +10_000.
                    *score = Score::cp(rng.gen_range(-10_000..10_000));
                }
            }
        }

        fn score_quiets(&mut self, quiets: Scorer) {
            let mut rng = rand::thread_rng();

            for (mov, score) in quiets {
                if mov.is_capture() {
                    // Assign a random number from -10_000 to +10_000.
                    *score = Score::cp(rng.gen_range(-10_000..10_000));
                }
            }
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
