//! History tables.

use core::position::{Player, Square};

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
    T: std::ops::AddAssign,
{
    /// Increment by `amt` a from-to pair on the butterfly board, indexed by the squares.
    ///
    /// # Panics
    ///
    /// This method will panic if the squares passed are not valid squares (i.e. they satisfy
    /// `square.is_okay() == true`).
    pub fn inc(&mut self, from: Square, to: Square, amt: T) {
        self.data[from.index() as usize][to.index() as usize] += amt;
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

/// A structure storing two butterfly tables of `u32`s, used to record the history value of moves
/// during search.
///
/// This data structure occupies about 32KB of memory.
#[derive(Debug)]
pub struct HistoryTable {
    white: Butterfly<u32>,
    black: Butterfly<u32>,
}

impl HistoryTable {
    pub fn new() -> Self {
        HistoryTable {
            white: Default::default(),
            black: Default::default(),
        }
    }

    pub fn inc(&mut self, from: Square, to: Square, amt: u32, side: Player) {
        match side {
            Player::WHITE => self.white.inc(from, to, amt),
            Player::BLACK => self.black.inc(from, to, amt),
        }
    }

    pub fn get(&self, from: Square, to: Square, side: Player) -> u32 {
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
    pub unsafe fn get_unchecked(&self, from: Square, to: Square, side: Player) -> u32 {
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
