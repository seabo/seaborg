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
        assert!(from.is_okay());
        assert!(to.is_okay());

        // SAFETY: bounds have been checked above.
        unsafe {
            *self
                .data
                .get_unchecked_mut(from.0 as usize)
                .get_unchecked_mut(to.0 as usize) += amt;
        }
    }

    /// Increment by `amt` a from-to pair on the butterfly board, indexed by the squares.
    ///
    /// This method assumes that the squares are valid (i.e. they have value < 64); in debug mode,
    /// the function will panic if this doesn't hold but in release mode, UB will occur as the
    /// check is elided.
    pub unsafe fn inc_unchecked(&mut self, from: Square, to: Square, amt: T) {
        debug_assert!(from.is_okay());
        debug_assert!(to.is_okay());

        *self
            .data
            .get_unchecked_mut(from.0 as usize)
            .get_unchecked_mut(to.0 as usize) += amt;
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
        assert!(from.is_okay());
        assert!(to.is_okay());

        // SAFETY: bounds have been checked above.
        unsafe {
            *self
                .data
                .get_unchecked(from.0 as usize)
                .get_unchecked(to.0 as usize)
        }
    }

    /// Get the value indexed by `from` and `to`.
    ///
    /// This method assumes that the squares are valid (i.e. they have value < 64); in debug mode,
    /// the function will panic if this doesn't hold but in release mode, UB will occur as the
    /// check is elided.
    pub unsafe fn get_unchecked(&self, from: Square, to: Square) -> T {
        debug_assert!(from.is_okay());
        debug_assert!(to.is_okay());

        // SAFETY: bounds have been checked above.
        *self
            .data
            .get_unchecked(from.0 as usize)
            .get_unchecked(to.0 as usize)
    }
}

/// A structure storing two butterfly tables of `u16`s, used to record the history value of moves
/// during search.
///
/// This data structure occupies about 16KB of memory.
#[derive(Debug)]
pub struct HistoryTable {
    white: Butterfly<u16>,
    black: Butterfly<u16>,
}

impl HistoryTable {
    pub fn new() -> Self {
        HistoryTable {
            white: Default::default(),
            black: Default::default(),
        }
    }

    pub unsafe fn inc_unchecked(&mut self, from: Square, to: Square, amt: u16, side: Player) {
        match side {
            Player::WHITE => self.white.inc_unchecked(from, to, amt),
            Player::BLACK => self.black.inc_unchecked(from, to, amt),
        }
    }

    pub unsafe fn get_unchecked(&self, from: Square, to: Square, side: Player) -> u16 {
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
