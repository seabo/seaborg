//! Contextual quiet-move evidence: counter moves and continuation history.
//!
//! Plain butterfly history scores a quiet move only by its origin and destination and by the side
//! to move. That cannot tell a move that is generally useful from one that is specifically a strong
//! reply to the move just played. The two tables here add that context by keying on the *preceding*
//! moves of the current line:
//!
//! * The [`CounterMoveTable`] keeps, for each preceding move, the single quiet reply that most
//!   recently refuted it — the classic counter-move heuristic.
//! * [`ContinuationHistory`] keeps a bounded, self-decaying score for a quiet move conditioned on a
//!   preceding move, for the move one ply back and the move two plies back.
//!
//! Both are keyed by a *(piece, to-square)* context rather than a bare from/to. The piece includes
//! its colour, so unlike plain history these tables need no separate per-side dimension: a white
//! knight landing on a square and a black knight landing on it index different rows already.
//!
//! Both tables are search-local: a Lazy SMP worker owns its own, retains them across
//! iterative-deepening iterations, and clears them between separate searches. Nothing here is shared
//! between workers.

use chess::mov::Move;
use chess::position::{Piece, Square};

use crate::history::gravity_update;

/// Number of *(piece, square)* contexts: twelve real pieces (colour included) times 64 squares.
/// `Piece::None` is excluded because every move on the board is made by a real piece.
const CONTEXTS: usize = 12 * 64;

/// Continuation distances tracked: the move one ply back (distance 0) and two plies back
/// (distance 1). Two is the mandated minimum; deeper distances would add another `CONTEXTS`-wide
/// grid each and are not tracked here.
pub const CONTINUATION_DISTANCES: usize = 2;

/// Index a *(piece, square)* pair into `0..CONTEXTS`.
///
/// `piece` must be a real piece, never `Piece::None`; both the mover of a preceding move and the
/// quiet being scored are real pieces on the board, so this always holds in the search.
#[inline(always)]
pub fn piece_to_index(piece: Piece, sq: Square) -> usize {
    debug_assert!(!piece.is_none());
    debug_assert!(sq.is_okay());
    (piece as usize - 1) * 64 + sq.index() as usize
}

/// One quiet reply per preceding move: the counter-move heuristic.
///
/// A store records "this quiet just refuted the move that reached the current node"; a probe of the
/// same context returns that reply so ordering can try it early. Because a counter learned against
/// one occurrence of a move is probed at a possibly different position, the caller must validate the
/// returned move for legality before executing it — this table stores identity, not legality.
#[derive(Debug)]
pub struct CounterMoveTable {
    /// One reply per *(piece, to)* context. `Move::null()` marks an empty context.
    moves: Box<[Move]>,
}

impl Default for CounterMoveTable {
    fn default() -> Self {
        Self::new()
    }
}

impl CounterMoveTable {
    pub fn new() -> Self {
        Self {
            moves: vec![Move::null(); CONTEXTS].into_boxed_slice(),
        }
    }

    /// The counter move stored against the preceding move `(prev_piece, prev_to)`, or
    /// `Move::null()` if none has been recorded. The returned move is not legality-checked here.
    #[inline]
    pub fn get(&self, prev_piece: Piece, prev_to: Square) -> Move {
        self.moves[piece_to_index(prev_piece, prev_to)]
    }

    /// Record `counter` as the reply to the preceding move `(prev_piece, prev_to)`, replacing any
    /// earlier reply. Replacement is pure recency: the latest refutation wins.
    #[inline]
    pub fn store(&mut self, prev_piece: Piece, prev_to: Square, counter: Move) {
        self.moves[piece_to_index(prev_piece, prev_to)] = counter;
    }

    /// Clear every context so a later search does not inherit replies learned for an unrelated
    /// position.
    pub fn reset(&mut self) {
        self.moves.fill(Move::null());
    }
}

/// Bounded continuation-history scores for a quiet move conditioned on a preceding move.
///
/// The evidence for continuation distance `d` lives in a `CONTEXTS x CONTEXTS` grid: the row is the
/// preceding move's *(piece, to)* context, the column is the quiet move's own *(piece, to)*. Every
/// update goes through the shared [`gravity_update`] rule, so the same bounded bonus/malus/aging
/// governs these scores as governs plain history — no independent unbounded counter is kept.
#[derive(Debug)]
pub struct ContinuationHistory {
    /// `CONTINUATION_DISTANCES` flattened `CONTEXTS x CONTEXTS` grids. Boxed as one slice rather
    /// than a fixed-size array so the ~4.7 MB allocation is built on the heap directly and never
    /// materialises on the stack.
    scores: Box<[i32]>,
}

impl Default for ContinuationHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinuationHistory {
    pub fn new() -> Self {
        Self {
            scores: vec![0; CONTINUATION_DISTANCES * CONTEXTS * CONTEXTS].into_boxed_slice(),
        }
    }

    /// Flatten a `(distance, context, current)` triple into the backing slice. All three operands
    /// are in range by construction: `dist < CONTINUATION_DISTANCES`, and both context indices come
    /// from [`piece_to_index`].
    #[inline(always)]
    fn index(dist: usize, ctx: usize, cur: usize) -> usize {
        debug_assert!(dist < CONTINUATION_DISTANCES);
        debug_assert!(ctx < CONTEXTS);
        debug_assert!(cur < CONTEXTS);
        (dist * CONTEXTS + ctx) * CONTEXTS + cur
    }

    /// The continuation score for playing `(cur_piece, cur_to)` after the preceding move
    /// `(prev_piece, prev_to)` at continuation distance `dist`, without bounds checks.
    ///
    /// # Safety
    ///
    /// `dist` must be less than [`CONTINUATION_DISTANCES`] and all pieces real; the search only ever
    /// calls this with a tracked distance and on-board pieces.
    #[inline(always)]
    pub unsafe fn get_unchecked(
        &self,
        dist: usize,
        prev_piece: Piece,
        prev_to: Square,
        cur_piece: Piece,
        cur_to: Square,
    ) -> i32 {
        let i = Self::index(
            dist,
            piece_to_index(prev_piece, prev_to),
            piece_to_index(cur_piece, cur_to),
        );
        debug_assert!(i < self.scores.len());
        *self.scores.get_unchecked(i)
    }

    /// Bounds-checked read, used by tests. The search hot path reads through
    /// [`ContinuationHistory::get_unchecked`].
    #[cfg(test)]
    pub fn get(
        &self,
        dist: usize,
        prev_piece: Piece,
        prev_to: Square,
        cur_piece: Piece,
        cur_to: Square,
    ) -> i32 {
        let i = Self::index(
            dist,
            piece_to_index(prev_piece, prev_to),
            piece_to_index(cur_piece, cur_to),
        );
        self.scores[i]
    }

    /// Apply a bounded continuation update for playing `(cur_piece, cur_to)` after
    /// `(prev_piece, prev_to)` at distance `dist`, through the shared [`gravity_update`] rule.
    #[inline]
    pub fn update(
        &mut self,
        dist: usize,
        prev_piece: Piece,
        prev_to: Square,
        cur_piece: Piece,
        cur_to: Square,
        bonus: i32,
    ) {
        let i = Self::index(
            dist,
            piece_to_index(prev_piece, prev_to),
            piece_to_index(cur_piece, cur_to),
        );
        gravity_update(&mut self.scores[i], bonus);
    }

    /// Reset every score to zero.
    pub fn reset(&mut self) {
        self.scores.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HISTORY_MAX;
    use chess::mov::MoveType;
    use chess::position::Square;

    fn quiet(orig: Square, dest: Square) -> Move {
        Move::build(orig, dest, None, MoveType::QUIET)
    }

    /// Every square on the board, built through the public constructor.
    fn all_squares() -> impl Iterator<Item = Square> {
        (0..8).flat_map(|rank| (0..8).map(move |file| Square::from_rank_file(rank, file)))
    }

    #[test]
    fn piece_to_index_is_dense_and_distinct() {
        // The two extreme pieces on the two extreme squares must map to the ends of the range and
        // nothing must land outside it.
        assert_eq!(piece_to_index(Piece::WhitePawn, Square::A1), 0);
        assert_eq!(piece_to_index(Piece::BlackKing, Square::H8), CONTEXTS - 1);

        let mut seen = std::collections::HashSet::new();
        for piece in [
            Piece::WhitePawn,
            Piece::WhiteKnight,
            Piece::WhiteBishop,
            Piece::WhiteRook,
            Piece::WhiteQueen,
            Piece::WhiteKing,
            Piece::BlackPawn,
            Piece::BlackKnight,
            Piece::BlackBishop,
            Piece::BlackRook,
            Piece::BlackQueen,
            Piece::BlackKing,
        ] {
            for sq in all_squares() {
                let idx = piece_to_index(piece, sq);
                assert!(idx < CONTEXTS);
                assert!(seen.insert(idx), "collision at {piece:?} {sq:?}");
            }
        }
        assert_eq!(seen.len(), CONTEXTS);
    }

    #[test]
    fn counter_move_round_trips_and_replaces_by_recency() {
        let mut table = CounterMoveTable::new();
        let first = quiet(Square::G1, Square::F3);
        let second = quiet(Square::B1, Square::C3);

        assert!(table.get(Piece::BlackPawn, Square::E5).is_null());

        table.store(Piece::BlackPawn, Square::E5, first);
        assert_eq!(table.get(Piece::BlackPawn, Square::E5), first);

        // A different context is untouched by the store above.
        assert!(table.get(Piece::BlackKnight, Square::E5).is_null());

        // The latest refutation replaces the earlier one.
        table.store(Piece::BlackPawn, Square::E5, second);
        assert_eq!(table.get(Piece::BlackPawn, Square::E5), second);

        table.reset();
        assert!(table.get(Piece::BlackPawn, Square::E5).is_null());
    }

    #[test]
    fn continuation_updates_are_bounded_and_context_local() {
        let mut cont = ContinuationHistory::new();
        let (pp, pt) = (Piece::BlackPawn, Square::E5);
        let (cp, ct) = (Piece::WhiteKnight, Square::F3);

        // A different continuation distance and a different context both stay zero while this one is
        // trained to saturation.
        for _ in 0..100 {
            cont.update(0, pp, pt, cp, ct, i32::MAX);
        }
        assert_eq!(cont.get(0, pp, pt, cp, ct), HISTORY_MAX);
        assert_eq!(cont.get(1, pp, pt, cp, ct), 0);
        assert_eq!(cont.get(0, Piece::WhitePawn, pt, cp, ct), 0);

        // Opposing evidence pulls the entry straight to the far bound, never past it.
        cont.update(0, pp, pt, cp, ct, -HISTORY_MAX);
        assert_eq!(cont.get(0, pp, pt, cp, ct), -HISTORY_MAX);

        cont.reset();
        assert_eq!(cont.get(0, pp, pt, cp, ct), 0);
    }
}
