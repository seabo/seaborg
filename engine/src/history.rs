//! History tables.

use chess::position::{Piece, PieceType, Player, Square};

/// Butterfly boards.
///
/// This is an implementation of a 64x64 table which can be index with two `Squares`. Note that
/// there is a lot of redundancy - only 1,792 of the 4,096 entries correspond to actual chess
/// moves.
///
/// In search, we would use _two_ butterfly boards, one for White and one for Black.
#[derive(Debug)]
pub struct Butterfly<T> {
    data: [[T; 64]; 64],
}

impl<T> Default for Butterfly<T>
where
    T: Default + Copy,
{
    fn default() -> Self {
        Butterfly {
            data: [[Default::default(); 64]; 64],
        }
    }
}

impl<T> Butterfly<T>
where
    T: Copy,
{
    /// Get the value indexed by `from` and `to`.
    ///
    /// # Panics
    ///
    /// This method will panic if the squares passed are not valid squares (i.e. they satisfy
    /// `square.is_okay() == true`).
    ///
    /// Only the tests use the bounds-checked accessor; the search hot path reads
    /// through [`Butterfly::get_unchecked`].
    #[cfg(test)]
    pub fn get(&self, from: Square, to: Square) -> T {
        self.data[from.index() as usize][to.index() as usize]
    }

    /// Get a value without bounds checks.
    ///
    /// # Safety
    ///
    /// Both squares must be in the range 0..64.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, from: Square, to: Square) -> T {
        debug_assert!(from.is_okay());
        debug_assert!(to.is_okay());

        *self
            .data
            .get_unchecked(from.index() as usize)
            .get_unchecked(to.index() as usize)
    }
}

/// Largest absolute history value.
///
/// This is deliberately one greater than `i16::MAX`: ordering scores must preserve the full
/// history value rather than silently wrapping a well-trained move below an untrained one.
pub const HISTORY_MAX: i32 = 32_768;

/// Apply one bounded, self-decaying history update to `entry`.
///
/// The gravity term `entry * |bonus| / HISTORY_MAX` makes repeated evidence progressively less
/// influential near either boundary and pulls stale evidence back toward zero when the sign of new
/// evidence changes. Clamping the requested bonus before the arithmetic keeps every intermediate
/// within `i32`, and the resulting entry is always in `-HISTORY_MAX..=HISTORY_MAX`.
///
/// This is the single bounded bonus/malus/aging rule shared by every quiet-move history table —
/// plain butterfly history, continuation history and any other contextual evidence — so that no
/// table accumulates unbounded or exposure-based counters of its own.
#[inline(always)]
pub fn gravity_update(entry: &mut i32, bonus: i32) {
    let bonus = bonus.clamp(-HISTORY_MAX, HISTORY_MAX);
    *entry += bonus - *entry * bonus.abs() / HISTORY_MAX;
}

/// A structure storing two butterfly tables of `i32`s, used to record the history value of moves
/// during search.
///
/// This data structure occupies about 32KB of memory.
#[derive(Debug)]
pub struct HistoryTable {
    white: Butterfly<i32>,
    black: Butterfly<i32>,
}

impl Default for HistoryTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryTable {
    pub fn new() -> Self {
        HistoryTable {
            white: Default::default(),
            black: Default::default(),
        }
    }

    /// Apply a bounded history update through the shared [`gravity_update`] rule.
    pub fn update(&mut self, from: Square, to: Square, bonus: i32, side: Player) {
        let entry = match side {
            Player::WHITE => &mut self.white.data[from.index() as usize][to.index() as usize],
            Player::BLACK => &mut self.black.data[from.index() as usize][to.index() as usize],
        };
        gravity_update(entry, bonus);
    }

    /// Read a history score with bounds-checked square indexing. Only the tests
    /// use this; the search hot path reads through [`HistoryTable::get_unchecked`].
    #[cfg(test)]
    pub fn get(&self, from: Square, to: Square, side: Player) -> i32 {
        match side {
            Player::WHITE => self.white.get(from, to),
            Player::BLACK => self.black.get(from, to),
        }
    }

    /// Get a history value without bounds checks.
    ///
    /// # Safety
    ///
    /// Both squares must be in the range 0..64.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, from: Square, to: Square, side: Player) -> i32 {
        match side {
            Player::WHITE => self.white.get_unchecked(from, to),
            Player::BLACK => self.black.get_unchecked(from, to),
        }
    }

    /// Reset the tables to zeros.
    pub fn reset(&mut self) {
        *self = Self::new()
    }
}

/// Number of moving-piece contexts: the twelve real pieces, colour included. Because the mover's
/// colour is part of the key, this table needs no separate per-side dimension.
const CAPTURE_MOVERS: usize = 12;

/// Number of captured piece types recorded, one per real piece type. A legal search only ever
/// captures pawn through queen, but the king slot is kept so the captured type maps directly onto
/// its index with no special case; it costs one plane that legal play never touches.
const CAPTURED_TYPES: usize = 6;

/// Bounded history for captures, keyed on the moving piece, the destination square and the type of
/// piece captured.
///
/// Static exchange evaluation says only whether a capture wins material on its square; it cannot
/// separate two captures with the same material outcome. This table supplies that missing signal by
/// recording how often a capture of a given (mover, destination, captured type) has produced a beta
/// cutoff, so ordering can try a capture the search has already found strong ahead of an untried one
/// of equal material value. Every update goes through the shared [`gravity_update`] rule, so the same
/// bounded bonus, malus and aging governs these scores as governs quiet history — no independent
/// unbounded counter is kept.
///
/// Like the other move-ordering tables it is search-local: a Lazy SMP worker owns its own, retains
/// it across iterative-deepening iterations and clears it between separate searches.
#[derive(Debug)]
pub struct CaptureHistory {
    /// A flattened `CAPTURE_MOVERS x 64 x CAPTURED_TYPES` grid. Boxed as one slice so the allocation
    /// lives on the heap rather than materialising on the stack inside [`super::search::Search`].
    scores: Box<[i32]>,
}

impl Default for CaptureHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureHistory {
    pub fn new() -> Self {
        Self {
            scores: vec![0; CAPTURE_MOVERS * 64 * CAPTURED_TYPES].into_boxed_slice(),
        }
    }

    /// Flatten a `(mover, destination, captured)` key into the backing slice.
    ///
    /// `mover` must be a real piece and `captured` a real piece type; both hold for every capture on
    /// the board, where a real piece moves onto a square occupied by a capturable piece (an
    /// en-passant capture keys its captured type as a pawn at the call site). A legal search never
    /// captures a king, so the king plane is exercised only by the illegal synthetic positions the
    /// move-ordering tests build for worst-case coverage.
    #[inline(always)]
    fn index(mover: Piece, dest: Square, captured: PieceType) -> usize {
        debug_assert!(!mover.is_none());
        debug_assert!(!captured.is_none());
        debug_assert!(dest.is_okay());
        let mover = mover as usize - 1;
        let captured = captured as usize - 1;
        (mover * 64 + dest.index() as usize) * CAPTURED_TYPES + captured
    }

    /// The capture-history score for playing `(mover, dest)` capturing `captured`, without bounds
    /// checks.
    ///
    /// # Safety
    ///
    /// `mover` must be a real piece and `captured` a real piece type; the search only ever reads this
    /// for a real capture on the board.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, mover: Piece, dest: Square, captured: PieceType) -> i32 {
        let i = Self::index(mover, dest, captured);
        debug_assert!(i < self.scores.len());
        *self.scores.get_unchecked(i)
    }

    /// Bounds-checked read, used by tests. The search hot path reads through
    /// [`CaptureHistory::get_unchecked`].
    #[cfg(test)]
    pub fn get(&self, mover: Piece, dest: Square, captured: PieceType) -> i32 {
        self.scores[Self::index(mover, dest, captured)]
    }

    /// Apply a bounded capture-history update for playing `(mover, dest)` capturing `captured`,
    /// through the shared [`gravity_update`] rule.
    #[inline]
    pub fn update(&mut self, mover: Piece, dest: Square, captured: PieceType, bonus: i32) {
        gravity_update(&mut self.scores[Self::index(mover, dest, captured)], bonus);
    }

    /// Reset every score to zero.
    pub fn reset(&mut self) {
        self.scores.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_updates_are_bounded_and_adapt_to_opposing_evidence() {
        let from = Square::A2;
        let to = Square::A3;
        let mut history = HistoryTable::new();

        for _ in 0..100 {
            history.update(from, to, i32::MAX, Player::WHITE);
        }
        assert_eq!(history.get(from, to, Player::WHITE), HISTORY_MAX);

        history.update(from, to, -HISTORY_MAX, Player::WHITE);
        assert_eq!(history.get(from, to, Player::WHITE), -HISTORY_MAX);

        for _ in 0..100 {
            history.update(from, to, i32::MIN, Player::WHITE);
        }
        assert_eq!(history.get(from, to, Player::WHITE), -HISTORY_MAX);
        assert_eq!(history.get(from, to, Player::BLACK), 0);
    }

    #[test]
    fn capture_history_updates_are_bounded_and_key_local() {
        let mut capture = CaptureHistory::new();
        let (mover, dest, captured) = (Piece::WhitePawn, Square::D5, PieceType::Knight);

        // Saturating a key leaves every other key — a different mover, destination or captured type —
        // untouched, and never pushes the trained entry past the bound.
        for _ in 0..100 {
            capture.update(mover, dest, captured, i32::MAX);
        }
        assert_eq!(capture.get(mover, dest, captured), HISTORY_MAX);
        assert_eq!(capture.get(Piece::WhiteKnight, dest, captured), 0);
        assert_eq!(capture.get(mover, Square::E5, captured), 0);
        assert_eq!(capture.get(mover, dest, PieceType::Bishop), 0);

        // Opposing evidence pulls the entry straight to the far bound, never past it — the aging that
        // lets the table adapt within a search.
        capture.update(mover, dest, captured, -HISTORY_MAX);
        assert_eq!(capture.get(mover, dest, captured), -HISTORY_MAX);

        capture.reset();
        assert_eq!(capture.get(mover, dest, captured), 0);
    }
}
