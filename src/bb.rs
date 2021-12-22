use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq)]
// TODO: internal u64 field should not be pub
pub struct Bitboard(u64);

impl Bitboard {
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
}

impl std::ops::BitAnd for Bitboard {
    type Output = Bitboard;

    fn bitand(self, other: Bitboard) -> Bitboard {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left & right),
        }
    }
}

impl std::ops::BitAndAssign for Bitboard {
    fn bitand_assign(&mut self, other: Bitboard) -> () {
        *self = *self & other;
    }
}

impl std::ops::BitOr for Bitboard {
    type Output = Bitboard;

    fn bitor(self, other: Bitboard) -> Bitboard {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left | right),
        }
    }
}

impl std::ops::BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, other: Bitboard) -> () {
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
                write!(f, " {} ", square)?;
            }
            write!(f, "│\n")?;
        }
        writeln!(f, "   └────────────────────────┘")?;
        writeln!(f, "     a  b  c  d  e  f  g  h ")
    }
}
