use crate::bit_twiddles::more_than_one;
use crate::masks::*;
use crate::position::Square;

use std::fmt;
use std::ops::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Bitboard(pub u64);

impl Bitboard {
    /// Bitboard of all squares
    pub const ALL: Bitboard = Bitboard(ALL);
    /// Bitboard of File A.
    pub const FILE_A: Bitboard = Bitboard(FILE_A);
    /// Bitboard of File A.
    pub const FILE_B: Bitboard = Bitboard(FILE_B);
    /// Bitboard of File A.
    pub const FILE_C: Bitboard = Bitboard(FILE_C);
    /// Bitboard of File A.
    pub const FILE_D: Bitboard = Bitboard(FILE_D);
    /// Bitboard of File A.
    pub const FILE_E: Bitboard = Bitboard(FILE_E);
    /// Bitboard of File A.
    pub const FILE_F: Bitboard = Bitboard(FILE_F);
    /// Bitboard of File A.
    pub const FILE_G: Bitboard = Bitboard(FILE_G);
    /// Bitboard of File A.
    pub const FILE_H: Bitboard = Bitboard(FILE_H);
    /// Bitboard Rank 1.
    pub const RANK_1: Bitboard = Bitboard(RANK_1);
    /// Bitboard Rank 1.
    pub const RANK_2: Bitboard = Bitboard(RANK_2);
    /// Bitboard Rank 1.
    pub const RANK_3: Bitboard = Bitboard(RANK_3);
    /// Bitboard Rank 1.
    pub const RANK_4: Bitboard = Bitboard(RANK_4);
    /// Bitboard Rank 1.
    pub const RANK_5: Bitboard = Bitboard(RANK_5);
    /// Bitboard Rank 1.
    pub const RANK_6: Bitboard = Bitboard(RANK_6);
    /// Bitboard Rank 1.
    pub const RANK_7: Bitboard = Bitboard(RANK_7);
    /// Bitboard Rank 1.
    pub const RANK_8: Bitboard = Bitboard(RANK_8);

    // TODO: rename this to `from()` - OR DELETE?
    pub fn new(bb: u64) -> Self {
        Bitboard(bb)
    }

    /// Produces a `Bitboard` with a single bit set at the index provided.
    #[inline(always)]
    pub fn from_sq_idx(sq: u8) -> Self {
        Bitboard(1 << sq)
    }

    /// Returns the count of set bits in the `Bitboard`.
    #[inline(always)]
    pub fn popcnt(&self) -> u32 {
        self.0.count_ones()
    }

    /// Returns the number of trailing zeros in the `Bitboard`. In the case where
    /// this is not 64 (ie the `Bitboard` is not empty), the return value of this
    /// function represents the index of the lowest set bit.
    // TODO: change this to return a `Square`. May need to be a panicking and
    // non-panicking version.
    #[inline(always)]
    pub fn bsf(&self) -> u32 {
        self.0.trailing_zeros()
    }

    /// Toggles off the current lowest significant bit which is set.
    #[inline(always)]
    pub fn toggle_lsb(&mut self) {
        *self &= *self - (1 as u64)
    }

    /// Returns true iff the `Bitboard` has no bits set.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Returns true iff the `Bitboard` has at least one bit set.
    #[inline(always)]
    pub fn is_not_empty(&self) -> bool {
        self.0 != 0
    }

    /// Returns if there are more than 1 bits inside.
    #[inline(always)]
    pub fn more_than_one(self) -> bool {
        more_than_one(self.0)
    }

    /// Returns the square for a given bitboard.
    ///
    /// # Panics
    ///
    /// In debug mode, panics if more than one bit is set.
    #[inline(always)]
    pub fn to_square(&self) -> Square {
        debug_assert!(self.popcnt() == 1);
        Square(self.bsf() as u8)
    }

    /// Returns the `Square` and `Bitboard` of the least significant bit and removes
    /// that bit from the `Bitboard`.
    ///
    /// # Safety
    ///
    /// Panics if the `Bitboard` is empty. See [`Bitboard::pop_some_lsb_and_bit`] for a
    /// non-panicking version of the method.
    #[inline(always)]
    pub fn pop_lsb_and_bit(&mut self) -> (Square, Bitboard) {
        let sq: Square = Square(self.bsf() as u8);
        *self &= *self - 1;
        (sq, sq.to_bb())
    }

    /// Returns the `Square` and `Bitboard` of the least significant bit and removes
    /// that bit from the `Bitboard`. If there are no bits left (the board is empty), returns
    /// `None`.
    #[inline(always)]
    pub fn pop_some_lsb_and_bit(&mut self) -> Option<(Square, Bitboard)> {
        if self.is_empty() {
            None
        } else {
            Some(self.pop_lsb_and_bit())
        }
    }
}

impl_bit_ops!(Bitboard, u64);

impl std::iter::Iterator for Bitboard {
    type Item = Square;

    #[inline(always)]
    fn next(&mut self) -> Option<Square> {
        match self.bsf() {
            64 => None,
            x => {
                self.toggle_lsb();
                Some(Square(x as u8))
            }
        }
    }
}

impl fmt::Display for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut squares: [[u8; 8]; 8] = [[0; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            let x: Bitboard = Bitboard(1 << i);
            if x & *self != Bitboard(0) {
                squares[rank][file] = 1;
            }
        }

        writeln!(f, "")?;
        writeln!(f, "   ┌────────────────────────┐")?;
        for (i, row) in squares.iter().rev().enumerate() {
            write!(f, " {} │", 8 - i)?;
            for square in row {
                if *square == 1 {
                    write!(f, " 1 ")?;
                } else {
                    write!(f, " . ")?;
                }
            }
            write!(f, "│\n")?;
        }
        writeln!(f, "   └────────────────────────┘")?;
        writeln!(f, "     a  b  c  d  e  f  g  h ")
    }
}
