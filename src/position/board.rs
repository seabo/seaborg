use super::{Piece, Square};
use std::fmt;

pub struct Board {
    pub arr: [Piece; 64],
}

impl Board {
    pub fn new() -> Self {
        Self {
            arr: [Piece::None; 64],
        }
    }

    pub fn from_array(board: [Piece; 64]) -> Self {
        Self { arr: board }
    }

    pub fn piece_at_sq(&self, sq: Square) -> Piece {
        assert!(sq.is_okay());
        unsafe { *self.arr.get_unchecked(sq.0 as usize) }
    }

    pub fn pretty_string(&self) -> String {
        let mut s = String::new();
        let mut squares: [[Piece; 8]; 8] = [[Piece::None; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            squares[rank][file] = self.arr[i]
        }

        s.push_str("   ┌────────────────────────┐\n");
        for (i, row) in squares.iter().rev().enumerate() {
            s.push_str(&format!(" {} │", 8 - i));
            for square in row {
                s.push_str(&format!(" {} ", square));
            }
            s.push_str("│\n");
        }
        s.push_str("   └────────────────────────┘\n");
        s.push_str("     a  b  c  d  e  f  g  h \n");

        s
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_string())
    }
}
