use crate::bit_twiddles::more_than_one;
use crate::masks::*;
use crate::position::Square;

use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct Bitboard(pub u64);

pub const WHITE_SINGLE_PAWN_MOVE_MASK: Bitboard = Bitboard(0x0000FFFFFFFFFF00);
pub const WHITE_LEFT_PAWN_CAPTURE_MASK: Bitboard = Bitboard(0x007F7F7F7F7F0000);
pub const WHITE_RIGHT_PAWN_CAPTURE_MASK: Bitboard = Bitboard(0x00FEFEFEFEFE0000);
pub const WHITE_LEFTWARD_PROMOTION_MASK: Bitboard = Bitboard(0x00FE000000000000);
pub const WHITE_RIGHTWARD_PROMOTION_MASK: Bitboard = Bitboard(0x007F000000000000);

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

    pub fn from_sq_idx(sq: u8) -> Self {
        Bitboard(1 << sq)
    }

    #[inline(always)]
    pub fn popcnt(&self) -> u32 {
        self.0.count_ones()
    }

    #[inline(always)]
    pub fn bsf(&self) -> u32 {
        self.0.trailing_zeros()
    }

    #[inline(always)]
    pub fn toggle_lsb(&mut self) {
        *self &= *self - (1 as u64)
    }

    #[inline]
    pub fn is_not_empty(&self) -> bool {
        self.0 != 0
    }

    /// Returns if there are more than 1 bits inside.
    #[inline(always)]
    pub fn more_than_one(self) -> bool {
        more_than_one(self.0)
    }

    /// Returns the square for a given bitboard. Panics if more than one
    /// bit is set.
    #[inline]
    pub fn to_square(&self) -> Square {
        assert!(self.popcnt() == 1);
        Square(self.bsf() as u8)
    }
}

impl std::ops::Add for Bitboard {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left + right),
        }
    }
}

impl std::ops::AddAssign for Bitboard {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other
    }
}

impl std::ops::Sub for Bitboard {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left - right),
        }
    }
}

impl std::ops::SubAssign for Bitboard {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other
    }
}

impl std::ops::Add<u64> for Bitboard {
    type Output = Self;

    fn add(self, other: u64) -> Self::Output {
        match self {
            Bitboard(left) => Bitboard(left + other),
        }
    }
}

impl std::ops::AddAssign<u64> for Bitboard {
    fn add_assign(&mut self, other: u64) {
        *self = *self + other
    }
}

impl std::ops::Sub<u64> for Bitboard {
    type Output = Self;

    fn sub(self, other: u64) -> Self::Output {
        match self {
            Bitboard(left) => Bitboard(left - other),
        }
    }
}

impl std::ops::SubAssign<u64> for Bitboard {
    fn sub_assign(&mut self, other: u64) {
        *self = *self - other
    }
}

impl std::ops::BitAnd for Bitboard {
    type Output = Self;

    fn bitand(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left & right),
        }
    }
}

impl std::ops::BitAndAssign for Bitboard {
    fn bitand_assign(&mut self, other: Self) {
        *self = *self & other;
    }
}

impl std::ops::BitOr for Bitboard {
    type Output = Self;

    fn bitor(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left | right),
        }
    }
}

impl std::ops::BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, other: Self) {
        *self = *self | other;
    }
}

impl std::ops::Not for Bitboard {
    type Output = Self;

    fn not(self) -> Bitboard {
        match self {
            Bitboard(bb) => Bitboard(!bb),
        }
    }
}

impl std::ops::Shl<usize> for Bitboard {
    type Output = Self;

    fn shl(self, shift: usize) -> Bitboard {
        match self {
            Bitboard(bb) => Bitboard(bb << shift),
        }
    }
}

impl std::ops::Shr<usize> for Bitboard {
    type Output = Self;

    fn shr(self, shift: usize) -> Self::Output {
        match self {
            Bitboard(bb) => Bitboard(bb >> shift),
        }
    }
}

impl std::iter::Iterator for Bitboard {
    type Item = Square;

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
