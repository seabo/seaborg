//! History tables.

use chess::position::{Player, Square};

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
}
