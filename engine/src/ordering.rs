//! Tools for ordering and iterating moves in a search environment.

use core::mov::Move;
use core::movelist::{ArrayVec, MoveList};
use core::position::PieceType;

use std::iter::Chain;
use std::ops::Range;

pub type ScoredMove = (Move, i16);

/// An `ArrayVec` containing `ScoredMoves`.
#[derive(Debug)]
pub struct ScoredMoveList(ArrayVec<ScoredMove, 254>);

/// An iterator over a `ScoredMoveList` which allows the `Move`s to be inspected and scores mutated.
pub struct Scorer<'a> {
    iter: std::slice::IterMut<'a, ScoredMove>,
}

impl<'a> Iterator for Scorer<'a> {
    type Item = (&'a Move, &'a mut i16);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|sm| (&sm.0, &mut sm.1))
    }
}

impl<'a> From<&'a mut [ScoredMove]> for Scorer<'a> {
    fn from(val: &'a mut [ScoredMove]) -> Self {
        Self {
            iter: val.iter_mut(),
        }
    }
}

impl MoveList for ScoredMoveList {
    fn empty() -> Self {
        ScoredMoveList(ArrayVec::new())
    }

    fn push(&mut self, mv: Move) {
        self.0.push_val((mv, 0));
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// Move every entry satisfying `pred` to the front of `segment`, preserving the relative order
/// within both the matching and the non-matching group. Returns the number of entries moved to the
/// front, so the two groups are `segment[..n]` and `segment[n..]`.
///
/// Stability is the point of this function rather than an incidental property. Every consumer of a
/// partitioned segment selects from it by taking the first maximum, so any reordering of
/// equal-scored moves here would change which of them the search tries first, and with it the shape
/// of the whole tree.
///
/// Each rotation shifts only the entries lying between the write head and the entry being moved,
/// which is the number of non-matching entries seen so far. Segments hold at most a few dozen moves
/// and usually far fewer, so this beats the alternatives: a linear-time stable partition needs a
/// scratch buffer the size of the segment, and an in-place one without extra space needs
/// O(n log n) rotations.
fn partition_front(segment: &mut [ScoredMove], pred: impl Fn(&ScoredMove) -> bool) -> usize {
    let mut front = 0;
    for i in 0..segment.len() {
        if pred(&segment[i]) {
            if i > front {
                segment[front..=i].rotate_right(1);
            }
            front += 1;
        }
    }
    front
}

/// A shrinking selection sort over a segment of scored moves.
///
/// Selection sort suits move ordering because sorting a whole segment up front is wasted effort in
/// the common case where an early move causes a cutoff. For the short lists involved, an O(n^2)
/// algorithm with a low constant factor also beats an O(n log n) one carrying more overhead.
///
/// The segment shrinks from the front as moves are taken, so an entry already yielded is never
/// looked at again and draining n moves costs about n^2/2 comparisons.
///
/// Two properties together fix the yielded order completely: the entry selected is the *first*
/// maximum among those remaining, and it is rotated to the front rather than swapped there.
/// Rotation leaves the entries it passes over in their original relative order, whereas a swap
/// would fling the displaced entry to the far end of the segment and silently reorder later ties.
/// The sequence yielded is therefore always the segment sorted by descending score with ties broken
/// by the order the moves were generated in.
struct SelectionSort<'a> {
    /// The entries not yet yielded.
    remaining: &'a mut [ScoredMove],
}

impl<'a> SelectionSort<'a> {
    fn new(segment: &'a mut [ScoredMove]) -> Self {
        Self { remaining: segment }
    }
}

impl Iterator for SelectionSort<'_> {
    type Item = Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (first, rest) = self.remaining.split_first()?;

        // Seeding the running maximum from the first remaining entry rather than a sentinel is what
        // lets a move be scored anywhere in `i16`, including `i16::MIN`. A sentinel seed would make
        // the lowest representable score unyieldable and impose an undocumented constraint on what
        // a `Loader` may assign.
        let mut best = 0;
        let mut best_score = first.1;
        for (i, sm) in rest.iter().enumerate() {
            if sm.1 > best_score {
                best = i + 1;
                best_score = sm.1;
            }
        }

        if best > 0 {
            self.remaining[..=best].rotate_right(1);
        }

        let mov = self.remaining[0].0;

        // Reborrow the tail for the iterator's own lifetime, which requires moving the slice out of
        // `self` rather than borrowing through it.
        let remaining = std::mem::take(&mut self.remaining);
        self.remaining = &mut remaining[1..];

        Some(mov)
    }
}

/// Promotions are yielded in two groups: those that capture, then those that do not. Each group is
/// sorted independently, which is why this is two selection sorts over adjacent subranges rather
/// than one sort with a compound key.
type PromotionsIter<'a> = Chain<SelectionSort<'a>, SelectionSort<'a>>;

/// An iterator over the killer moves.
///
/// Killers carry no score. They are yielded in the order the killer table returned them, which is
/// already a ranking, so there is nothing here to sort.
struct KillerIter<'a> {
    killers: std::slice::Iter<'a, ScoredMove>,
}

impl Iterator for KillerIter<'_> {
    type Item = Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.killers.next().map(|sm| sm.0)
    }
}

/// The buffer range each ordering phase draws its moves from. Every range indexes
/// `OrderedMoves::buf`.
///
/// Phases that yield capturing moves ahead of quiet ones own two adjacent subranges, which is why
/// these are named fields rather than one array indexed by `Phase`.
#[derive(Debug, Default)]
struct Segments {
    hash: Range<usize>,
    promo_capts: Range<usize>,
    promo_quiets: Range<usize>,
    good_capts: Range<usize>,
    equal_capts: Range<usize>,
    bad_capts: Range<usize>,
    killers: Range<usize>,
    quiets: Range<usize>,
    underpromo_capts: Range<usize>,
    underpromo_quiets: Range<usize>,
}

/// A structure for managing move ordering during search.
///
/// Whenever movegen is required, create a new `OrderedMoves`. To use it, the caller must also have
/// a type which implements `Loader`. This allows `OrderedMoves` to do staged loading of moves, and
/// to receive scores for captures (usually, the `Loader` will run SEE on the captures) and scores
/// for quiet moves (usually, the `Loader` will consult a history table).
///
/// Moves are generated in phases, so each new phase requires a call to
/// `OrderedMoves::load_next_phase`, passing a `Loader`. If this returns `true`, then there are more
/// moves to be yielded, and `&mut OrderedMoves` is `IntoIterator` over them. Iterating consumes:
/// selection reorders the buffer in place and each phase's moves can only be drawn once. The
/// iterator is produced from a mutable borrow so that this is visible in the type rather than being
/// a trap for the caller.
///
/// `OrderedMoves` is built on top of an `ArrayVec`, as this appears to be significantly more
/// performant than any solution involving overflows or allocations, since there is very little
/// overhead / pointer chasing / bounds checking to manage an `ArrayVec`. However, the downside is
/// that `OrderedMoves` is a large structure - currently 1704 bytes. We will have one of these at
/// every ply, so if we are searching deeply we could reach 100KB of data on the stack, just for
/// move ordering structs.
pub struct OrderedMoves {
    buf: ScoredMoveList,
    /// The index of the start of the segment currently being loaded. A new segment is created each
    /// time moves are appended to the buffer: one for the hash move, one for promotions, one for
    /// the underpromotions derived from them, one for captures, one for killers and one for quiets.
    segment_start: usize,
    segments: Segments,
    phase: Phase,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
        *self = match *self {
            Phase::Pre => Phase::HashTable,
            Phase::HashTable => Phase::QueenPromotions,
            Phase::QueenPromotions => Phase::GoodCaptures,
            Phase::GoodCaptures => Phase::EqualCaptures,
            Phase::EqualCaptures => Phase::Killers,
            Phase::Killers => Phase::Quiet,
            Phase::Quiet => Phase::BadCaptures,
            Phase::BadCaptures => Phase::Underpromotions,
            Phase::Underpromotions => return false,
        };
        true
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

impl Default for OrderedMoves {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderedMoves {
    pub fn new() -> Self {
        Self {
            buf: ScoredMoveList::empty(),
            segment_start: 0,
            segments: Segments::default(),
            phase: Phase::Pre,
        }
    }

    pub fn next_phase(&self) -> Phase {
        self.phase
    }

    /// Close the segment that was just appended to the buffer and return its range, assuming that
    /// it starts at `self.segment_start` and ends at `self.buf.len()`. This therefore assumes that
    /// it is being called immediately after the relevant moves have been loaded.
    fn close_segment(&mut self) -> Range<usize> {
        let range = self.segment_start..self.buf.len();
        self.segment_start = self.buf.len();
        range
    }

    /// The buffer entries in `range`.
    #[inline]
    fn segment(&mut self, range: Range<usize>) -> &mut [ScoredMove] {
        &mut self.buf.0.as_mut_slice()[range]
    }

    /// The two adjacent subranges of one segment, as disjoint slices.
    #[inline]
    fn split_segment(
        &mut self,
        first: Range<usize>,
        second: Range<usize>,
    ) -> (&mut [ScoredMove], &mut [ScoredMove]) {
        debug_assert_eq!(first.end, second.start);
        let len = first.len();
        self.segment(first.start..second.end).split_at_mut(len)
    }

    /// Move every entry of `seg` that also appears in one of `filters` to the back of `seg`, and
    /// return the range the surviving entries now occupy.
    ///
    /// A move already yielded in an earlier phase must not be searched a second time. Segregating
    /// such moves is what lets every phase iterator be a plain selection sort with nothing to skip,
    /// and it costs one pass over the segment rather than a test on every move yielded from it.
    ///
    /// The duplicates are pushed to the back rather than dropped because a segment can outlive the
    /// phase that yields from it: underpromotions are derived from the queen promotions, including
    /// from one that duplicates the hash move, whose underpromoting siblings are ordinary moves
    /// that still have to be searched.
    fn segregate_duplicates(
        &mut self,
        seg: Range<usize>,
        filters: &[Range<usize>],
    ) -> Range<usize> {
        let buf = self.buf.0.as_mut_slice();
        let mut front = seg.start;

        for i in seg.clone() {
            let mov = buf[i].0;
            let duplicate = filters
                .iter()
                .flat_map(|filter| filter.clone())
                .any(|j| buf[j].0 == mov);

            if !duplicate {
                if i > front {
                    buf[front..=i].rotate_right(1);
                }
                front += 1;
            }
        }

        seg.start..front
    }

    /// Split `seg` so that the moves which capture occupy the leading subrange, and return the two
    /// subranges in the order they are to be yielded.
    fn split_capturing_first(&mut self, seg: Range<usize>) -> (Range<usize>, Range<usize>) {
        let capts = partition_front(self.segment(seg.clone()), |sm| sm.0.is_capture());
        let mid = seg.start + capts;
        (seg.start..mid, mid..seg.end)
    }

    /// Load the next phase of moves into the buffer.
    ///
    /// This function calls methods on the passed in `loader` to fill the buffer with the moves
    /// need in the next phase.
    #[inline]
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
                    self.segments.hash = self.close_segment();
                }
                QueenPromotions => {
                    loader.load_promotions(&mut self.buf);
                    let promos = self.close_segment();

                    // Underpromotions are derived here, while the queen promotions are still in the
                    // order they were generated in. The phases below reorder that segment in place
                    // as they select from it, so deriving them later would make the underpromotion
                    // order depend on how the promotion phase happened to sort.
                    let underpromos = self.expand_underpromotions(promos.clone());

                    let hash = self.segments.hash.clone();
                    let promos = self.segregate_duplicates(promos, std::slice::from_ref(&hash));
                    (self.segments.promo_capts, self.segments.promo_quiets) =
                        self.split_capturing_first(promos);

                    let underpromos = self.segregate_duplicates(underpromos, &[hash]);
                    (
                        self.segments.underpromo_capts,
                        self.segments.underpromo_quiets,
                    ) = self.split_capturing_first(underpromos);
                }
                GoodCaptures => {
                    loader.load_captures(&mut self.buf);
                    let capts = self.close_segment();

                    loader.score_captures(self.segment(capts.clone()).into());

                    let hash = self.segments.hash.clone();
                    let capts = self.segregate_duplicates(capts, &[hash]);

                    // One partition here replaces a rescan of the whole capture segment by each of
                    // the three capture phases.
                    let good = partition_front(self.segment(capts.clone()), |sm| sm.1 > 0);
                    let good_end = capts.start + good;
                    let equal = partition_front(self.segment(good_end..capts.end), |sm| sm.1 == 0);
                    let equal_end = good_end + equal;

                    self.segments.good_capts = capts.start..good_end;
                    self.segments.equal_capts = good_end..equal_end;
                    self.segments.bad_capts = equal_end..capts.end;
                }
                EqualCaptures => { /* Nothing to do here */ }
                Killers => {
                    loader.load_killers(&mut self.buf);
                    let killers = self.close_segment();

                    let hash = self.segments.hash.clone();
                    self.segments.killers = self.segregate_duplicates(killers, &[hash]);
                }
                Quiet => {
                    loader.load_quiets(&mut self.buf);
                    let quiets = self.close_segment();

                    loader.score_quiets(self.segment(quiets.clone()).into());

                    let hash = self.segments.hash.clone();
                    let killers = self.segments.killers.clone();
                    self.segments.quiets = self.segregate_duplicates(quiets, &[hash, killers]);
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

    /// Append the rook, knight and bishop promotions corresponding to each queen promotion in
    /// `promos`, and return the range they were appended to.
    fn expand_underpromotions(&mut self, promos: Range<usize>) -> Range<usize> {
        for idx in promos {
            let mov = self.buf.0[idx].0;
            for piece_type in [PieceType::Rook, PieceType::Knight, PieceType::Bishop] {
                self.buf.push(mov.set_promo_type(piece_type));
            }
        }

        self.close_segment()
    }
}

enum IterInner<'a> {
    Empty,
    Hash(SelectionSort<'a>),
    QueenPromotions(PromotionsIter<'a>),
    GoodCaptures(SelectionSort<'a>),
    EqualCaptures(SelectionSort<'a>),
    Killers(KillerIter<'a>),
    Quiet(SelectionSort<'a>),
    BadCaptures(SelectionSort<'a>),
    Underpromotions(PromotionsIter<'a>),
}

pub struct PhaseIter<'a>(IterInner<'a>);

impl Iterator for PhaseIter<'_> {
    type Item = Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        use IterInner::*;
        match &mut self.0 {
            Empty => None,
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

impl<'a> IntoIterator for &'a mut OrderedMoves {
    type Item = Move;
    type IntoIter = PhaseIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        use Phase::*;
        let iter = match self.phase {
            Pre => IterInner::Empty,
            HashTable => {
                let hash = self.segments.hash.clone();
                IterInner::Hash(SelectionSort::new(self.segment(hash)))
            }
            QueenPromotions => {
                let (capts, quiets) = (
                    self.segments.promo_capts.clone(),
                    self.segments.promo_quiets.clone(),
                );
                let (capts, quiets) = self.split_segment(capts, quiets);
                IterInner::QueenPromotions(
                    SelectionSort::new(capts).chain(SelectionSort::new(quiets)),
                )
            }
            GoodCaptures => {
                let good = self.segments.good_capts.clone();
                IterInner::GoodCaptures(SelectionSort::new(self.segment(good)))
            }
            EqualCaptures => {
                let equal = self.segments.equal_capts.clone();
                IterInner::EqualCaptures(SelectionSort::new(self.segment(equal)))
            }
            Killers => {
                let killers = self.segments.killers.clone();
                IterInner::Killers(KillerIter {
                    killers: self.segment(killers).iter(),
                })
            }
            Quiet => {
                let quiets = self.segments.quiets.clone();
                IterInner::Quiet(SelectionSort::new(self.segment(quiets)))
            }
            BadCaptures => {
                let bad = self.segments.bad_capts.clone();
                IterInner::BadCaptures(SelectionSort::new(self.segment(bad)))
            }
            Underpromotions => {
                let (capts, quiets) = (
                    self.segments.underpromo_capts.clone(),
                    self.segments.underpromo_quiets.clone(),
                );
                let (capts, quiets) = self.split_segment(capts, quiets);
                IterInner::Underpromotions(
                    SelectionSort::new(capts).chain(SelectionSort::new(quiets)),
                )
            }
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
                for mov in &mut moves {
                    c += 1;
                    // SAFETY: `mov` was generated for the current test position.
                    unsafe { p.pos.make_move_unchecked(&mov) };
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
                    for mov in &mut moves {
                        // SAFETY: `mov` was generated for this position above.
                        unsafe { self.pos.make_move_unchecked(&mov) };
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
            if let Some(mv) = self.pos.generate::<BasicMoveList, All, Legal>().random() {
                movelist.push(*mv);
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
                    *score = rng.gen_range(-10_000..10_000);
                }
            }
        }

        fn score_quiets(&mut self, quiets: Scorer) {
            let mut rng = rand::thread_rng();

            for (mov, score) in quiets {
                if mov.is_capture() {
                    // Assign a random number from -10_000 to +10_000.
                    *score = rng.gen_range(-10_000..10_000);
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

    /// The documented size in the `OrderedMoves` doc comment is a claim about the stack cost of
    /// searching deeply, and one of these lives at every ply. Pin it so the comment cannot drift
    /// away from the type, and so a field added here has to be a deliberate choice.
    #[test]
    fn ordered_moves_size_matches_its_documentation() {
        assert_eq!(std::mem::size_of::<OrderedMoves>(), 1704);
    }

    /// A handful of distinct legal moves to hang synthetic scores off. Their identity does not
    /// matter to selection; only that they are distinguishable.
    fn sample_moves(n: usize) -> Vec<Move> {
        core::init::init_globals();
        let pos = Position::start_pos();
        let moves = pos.generate::<BasicMoveList, All, Legal>();
        let moves: Vec<Move> = moves.iter().copied().take(n).collect();
        assert_eq!(
            moves.len(),
            n,
            "start position has fewer moves than asked for"
        );
        moves
    }

    fn scored(scores: &[i16]) -> Vec<ScoredMove> {
        sample_moves(scores.len())
            .into_iter()
            .zip(scores.iter().copied())
            .collect()
    }

    /// Selection yields a segment in descending score order, breaking ties towards the move that
    /// was generated first.
    ///
    /// This is the property the search's node counts depend on: an ordering change that merely
    /// permutes equal-scored moves still reshapes the tree, because a different first move changes
    /// where cutoffs land. It is also the property an in-place selection is easiest to lose, since
    /// swapping the selected entry to the front would displace an unyielded one past its peers.
    #[test]
    fn selection_yields_by_score_then_generation_order() {
        let score_sets: &[&[i16]] = &[
            &[0, 0, 0, 0, 0],
            &[5, 3, 5, 1, 5],
            &[1, 2, 3, 4, 5],
            &[5, 4, 3, 2, 1],
            &[3, 3, 5, 3, 3, 5],
            &[i16::MIN, 0, i16::MAX, i16::MIN, i16::MAX],
        ];

        for scores in score_sets {
            let mut segment = scored(scores);

            let mut expected = segment.clone();
            expected.sort_by_key(|sm| std::cmp::Reverse(sm.1));
            let expected: Vec<Move> = expected.into_iter().map(|sm| sm.0).collect();

            let yielded: Vec<Move> = SelectionSort::new(&mut segment).collect();

            assert_eq!(yielded, expected, "scores {scores:?}");
        }
    }

    /// The lowest representable score is an ordinary score, not a sentinel. A `Loader` is free to
    /// assign it, and a move carrying it must still be yielded.
    #[test]
    fn a_move_scored_i16_min_is_still_yielded() {
        let mut segment = scored(&[i16::MIN]);
        let yielded: Vec<Move> = SelectionSort::new(&mut segment).collect();

        assert_eq!(yielded.len(), 1);
        assert_eq!(yielded[0], segment[0].0);
    }

    /// Partitioning keeps both groups in their incoming order, which is what lets selection over
    /// each group reproduce the tie-breaking of a selection over the whole segment.
    #[test]
    fn partitioning_preserves_order_within_both_groups() {
        let scores = [4, -1, 0, 7, -3, 0, 2];
        let mut segment = scored(&scores);
        let before = segment.clone();

        let front = partition_front(&mut segment, |sm| sm.1 > 0);

        let expected_front: Vec<ScoredMove> =
            before.iter().copied().filter(|sm| sm.1 > 0).collect();
        let expected_back: Vec<ScoredMove> =
            before.iter().copied().filter(|sm| sm.1 <= 0).collect();

        assert_eq!(front, expected_front.len());
        assert_eq!(&segment[..front], expected_front.as_slice());
        assert_eq!(&segment[front..], expected_back.as_slice());
    }

    /// A `Loader` whose every phase is dictated by the test.
    #[derive(Clone, Default)]
    struct ScriptedLoader {
        hash: Vec<Move>,
        promotions: Vec<Move>,
        captures: Vec<Move>,
        capture_scores: Vec<i16>,
        killers: Vec<Move>,
        quiets: Vec<Move>,
    }

    impl Loader for ScriptedLoader {
        fn load_hash(&mut self, movelist: &mut ScoredMoveList) {
            self.hash.iter().for_each(|m| movelist.push(*m));
        }

        fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
            self.promotions.iter().for_each(|m| movelist.push(*m));
        }

        fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
            self.captures.iter().for_each(|m| movelist.push(*m));
        }

        fn load_killers(&mut self, movelist: &mut ScoredMoveList) {
            self.killers.iter().for_each(|m| movelist.push(*m));
        }

        fn load_quiets(&mut self, movelist: &mut ScoredMoveList) {
            self.quiets.iter().for_each(|m| movelist.push(*m));
        }

        fn score_captures(&mut self, captures: Scorer) {
            for ((_, score), assigned) in captures.zip(self.capture_scores.iter()) {
                *score = *assigned;
            }
        }
    }

    /// Drain every phase, returning what each one yielded.
    fn drain_phases(loader: &ScriptedLoader) -> Vec<(Phase, Vec<Move>)> {
        let mut moves = OrderedMoves::new();
        let mut yielded = Vec::new();

        while moves.load_next_phase(loader.clone()) {
            let phase = moves.phase();
            yielded.push((phase, (&mut moves).into_iter().collect()));
        }

        yielded
    }

    fn phase_moves(phases: &[(Phase, Vec<Move>)], wanted: Phase) -> &[Move] {
        phases
            .iter()
            .find(|(phase, _)| *phase == wanted)
            .map(|(_, moves)| moves.as_slice())
            .expect("phase was never loaded")
    }

    /// Captures are split by static exchange evaluation once, and each capture phase draws only
    /// from its own share. A capture is yielded by exactly one phase, and within a phase the
    /// stronger capture comes first.
    #[test]
    fn each_capture_phase_yields_only_its_own_share() {
        let captures = sample_moves(6);
        let loader = ScriptedLoader {
            captures: captures.clone(),
            // Two winning, two neutral and two losing, interleaved so that a phase drawing from the
            // whole segment would be visible as an out-of-group move.
            capture_scores: vec![0, 300, -200, 100, -50, 0],
            ..Default::default()
        };

        let phases = drain_phases(&loader);

        assert_eq!(
            phase_moves(&phases, Phase::GoodCaptures),
            &[captures[1], captures[3]]
        );
        assert_eq!(
            phase_moves(&phases, Phase::EqualCaptures),
            &[captures[0], captures[5]]
        );
        assert_eq!(
            phase_moves(&phases, Phase::BadCaptures),
            &[captures[4], captures[2]]
        );
    }

    /// A move already yielded as the hash move is not yielded again by a later phase, whichever
    /// segment it also appears in.
    #[test]
    fn a_move_yielded_as_the_hash_move_is_not_yielded_again() {
        let moves = sample_moves(8);
        let loader = ScriptedLoader {
            hash: vec![moves[2], moves[5]],
            captures: moves[0..4].to_vec(),
            capture_scores: vec![0; 4],
            killers: vec![moves[4], moves[5]],
            quiets: moves[4..8].to_vec(),
            ..Default::default()
        };

        let phases = drain_phases(&loader);
        let all: Vec<Move> = phases.iter().flat_map(|(_, m)| m.iter().copied()).collect();

        assert_eq!(all.iter().filter(|m| **m == moves[2]).count(), 1);
        assert_eq!(all.iter().filter(|m| **m == moves[5]).count(), 1);
        assert_eq!(phase_moves(&phases, Phase::Killers), &[moves[4]]);
        // The killer already yielded is dropped from the quiets too, as is the hash move.
        assert_eq!(phase_moves(&phases, Phase::Quiet), &[moves[6], moves[7]]);
    }

    /// The queen promotion that duplicates the hash move is not yielded twice, but its
    /// underpromoting siblings are ordinary moves that must still be searched, and in the same
    /// order as if it had never been the hash move.
    #[test]
    fn underpromotions_survive_a_queen_promotion_that_duplicates_the_hash_move() {
        core::init::init_globals();
        // Both b7xa8 and b7xc8 promote with a capture; b7b8 promotes quietly. The white king is off
        // the a-file so that the rook on a8 does not give check and restrict the legal moves.
        let pos = Position::from_fen("r1r4k/1P6/8/8/8/8/8/6K1 w - - 0 1").unwrap();
        let promos: Vec<Move> = pos
            .generate::<BasicMoveList, QueenPromotions, Legal>()
            .iter()
            .copied()
            .collect();
        assert_eq!(
            promos.len(),
            3,
            "expected three queen promotions: {promos:?}"
        );

        let without_hash = drain_phases(&ScriptedLoader {
            promotions: promos.clone(),
            ..Default::default()
        });
        assert_eq!(
            phase_moves(&without_hash, Phase::Underpromotions).len(),
            promos.len() * 3
        );

        for hash in &promos {
            let with_hash = drain_phases(&ScriptedLoader {
                hash: vec![*hash],
                promotions: promos.clone(),
                ..Default::default()
            });

            assert_eq!(
                phase_moves(&with_hash, Phase::Underpromotions),
                phase_moves(&without_hash, Phase::Underpromotions),
                "underpromotion order changed when {hash} was the hash move"
            );
        }
    }
}
