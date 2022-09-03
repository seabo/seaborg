use crate::bb::Bitboard;
use crate::bit_twiddles::diff;
use std::fmt;
use std::ops::*;

/// Represents a single square of a chess board.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Square(pub u8);

impl_bit_ops!(Square, u8);

impl Square {
    /// Creates a square from a rank and a file. This is slow because it performs assertions to
    /// ensure that the rank and file are within bounds. It should never be needed in hot engine
    /// paths, just in places like parsing notation.
    pub fn from_rank_file(rank: usize, file: usize) -> Self {
        assert!(rank <= 7);
        assert!(file <= 7);
        Square((rank * 8 + file) as u8)
    }

    #[inline]
    pub const fn is_okay(&self) -> bool {
        self.0 < 64
    }

    #[inline]
    pub fn distance(&self, other: Self) -> u8 {
        let x = diff(self.rank_idx_of_sq(), other.rank_idx_of_sq());
        let y = diff(self.file_idx_of_sq(), other.file_idx_of_sq());
        if x > y {
            x
        } else {
            y
        }
    }

    /// Returns the rank index (number) of a `SQ`.
    #[inline(always)]
    pub const fn rank_idx_of_sq(self) -> u8 {
        (self.0 >> 3) as u8
    }

    /// Returns the file index (number) of a `SQ`.
    #[inline(always)]
    pub const fn file_idx_of_sq(self) -> u8 {
        (self.0 & 0b0000_0111) as u8
    }

    /// Returns the rank that the square lies on.
    #[inline]
    pub fn rank(self) -> u8 {
        (self.0 >> 3) & 0b0000_0111
    }

    /// Returns the file that the square lies on.
    #[inline]
    pub fn file(self) -> u8 {
        self.0 & 0b0000_0111
    }

    /// Converts the given `Square` to its equivalent `Bitboard`.
    #[inline]
    pub fn to_bb(self) -> Bitboard {
        Bitboard((1 as u64).wrapping_shl(self.0 as u32))
    }
}

// constants
impl Square {
    pub const A1: Square = Square(0b000000);
    #[doc(hidden)]
    pub const B1: Square = Square(0b000001);
    #[doc(hidden)]
    pub const C1: Square = Square(0b000010);
    #[doc(hidden)]
    pub const D1: Square = Square(0b000011);
    #[doc(hidden)]
    pub const E1: Square = Square(0b000100);
    #[doc(hidden)]
    pub const F1: Square = Square(0b000101);
    #[doc(hidden)]
    pub const G1: Square = Square(0b000110);
    #[doc(hidden)]
    pub const H1: Square = Square(0b000111);
    #[doc(hidden)]
    pub const A2: Square = Square(0b001000);
    #[doc(hidden)]
    pub const B2: Square = Square(0b001001);
    #[doc(hidden)]
    pub const C2: Square = Square(0b001010);
    #[doc(hidden)]
    pub const D2: Square = Square(0b001011);
    #[doc(hidden)]
    pub const E2: Square = Square(0b001100);
    #[doc(hidden)]
    pub const F2: Square = Square(0b001101);
    #[doc(hidden)]
    pub const G2: Square = Square(0b001110);
    #[doc(hidden)]
    pub const H2: Square = Square(0b001111);
    #[doc(hidden)]
    pub const A3: Square = Square(0b010000);
    #[doc(hidden)]
    pub const B3: Square = Square(0b010001);
    #[doc(hidden)]
    pub const C3: Square = Square(0b010010);
    #[doc(hidden)]
    pub const D3: Square = Square(0b010011);
    #[doc(hidden)]
    pub const E3: Square = Square(0b010100);
    #[doc(hidden)]
    pub const F3: Square = Square(0b010101);
    #[doc(hidden)]
    pub const G3: Square = Square(0b010110);
    #[doc(hidden)]
    pub const H3: Square = Square(0b010111);
    #[doc(hidden)]
    pub const A4: Square = Square(0b011000);
    #[doc(hidden)]
    pub const B4: Square = Square(0b011001);
    #[doc(hidden)]
    pub const C4: Square = Square(0b011010);
    #[doc(hidden)]
    pub const D4: Square = Square(0b011011);
    #[doc(hidden)]
    pub const E4: Square = Square(0b011100);
    #[doc(hidden)]
    pub const F4: Square = Square(0b011101);
    #[doc(hidden)]
    pub const G4: Square = Square(0b011110);
    #[doc(hidden)]
    pub const H4: Square = Square(0b011111);
    #[doc(hidden)]
    pub const A5: Square = Square(0b100000);
    #[doc(hidden)]
    pub const B5: Square = Square(0b100001);
    #[doc(hidden)]
    pub const C5: Square = Square(0b100010);
    #[doc(hidden)]
    pub const D5: Square = Square(0b100011);
    #[doc(hidden)]
    pub const E5: Square = Square(0b100100);
    #[doc(hidden)]
    pub const F5: Square = Square(0b100101);
    #[doc(hidden)]
    pub const G5: Square = Square(0b100110);
    #[doc(hidden)]
    pub const H5: Square = Square(0b100111);
    #[doc(hidden)]
    pub const A6: Square = Square(0b101000);
    #[doc(hidden)]
    pub const B6: Square = Square(0b101001);
    #[doc(hidden)]
    pub const C6: Square = Square(0b101010);
    #[doc(hidden)]
    pub const D6: Square = Square(0b101011);
    #[doc(hidden)]
    pub const E6: Square = Square(0b101100);
    #[doc(hidden)]
    pub const F6: Square = Square(0b101101);
    #[doc(hidden)]
    pub const G6: Square = Square(0b101110);
    #[doc(hidden)]
    pub const H6: Square = Square(0b101111);
    #[doc(hidden)]
    pub const A7: Square = Square(0b110000);
    #[doc(hidden)]
    pub const B7: Square = Square(0b110001);
    #[doc(hidden)]
    pub const C7: Square = Square(0b110010);
    #[doc(hidden)]
    pub const D7: Square = Square(0b110011);
    #[doc(hidden)]
    pub const E7: Square = Square(0b110100);
    #[doc(hidden)]
    pub const F7: Square = Square(0b110101);
    #[doc(hidden)]
    pub const G7: Square = Square(0b110110);
    #[doc(hidden)]
    pub const H7: Square = Square(0b110111);
    #[doc(hidden)]
    pub const A8: Square = Square(0b111000);
    #[doc(hidden)]
    pub const B8: Square = Square(0b111001);
    #[doc(hidden)]
    pub const C8: Square = Square(0b111010);
    #[doc(hidden)]
    pub const D8: Square = Square(0b111011);
    #[doc(hidden)]
    pub const E8: Square = Square(0b111100);
    #[doc(hidden)]
    pub const F8: Square = Square(0b111101);
    #[doc(hidden)]
    pub const G8: Square = Square(0b111110);
    #[doc(hidden)]
    pub const H8: Square = Square(0b111111);
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Square(idx) = self;
        let rank = idx / 8 + 1;
        let file = idx % 8;

        let file_name = match file {
            0 => "a",
            1 => "b",
            2 => "c",
            3 => "d",
            4 => "e",
            5 => "f",
            6 => "g",
            7 => "h",
            _ => panic!(
                "error: square struct has idx {} and file index {}",
                idx, file
            ),
        };

        write!(f, "{}{}", file_name, rank.to_string())
    }
}
