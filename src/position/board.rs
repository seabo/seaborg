use super::{Piece, PieceType, Player, Square};
use std::fmt;

#[derive(Clone, Eq, PartialEq)]
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

    /// Returns the Piece at a `Square`, Or None if the square is empty.
    ///
    /// Uses unchecked access to the array. Only checks that the `Square` is
    /// legitimate in debug mode.
    pub fn piece_at_sq(&self, sq: Square) -> Piece {
        debug_assert!(sq.is_okay());
        unsafe { *self.arr.get_unchecked(sq.0 as usize) }
    }

    /// Remove the piece at the passed `Square`.
    ///
    /// # Panics
    ///
    /// In debug mode, panics if the passed `Square` is not valid.
    pub fn remove(&mut self, sq: Square) {
        debug_assert!(sq.is_okay());
        self.arr[sq.0 as usize] = Piece::None;
    }

    /// Place a piece of type `PieceType` and color `Player` at the passed `Sqaure`.
    ///
    /// # Panics
    ///
    /// In debug mode, panics if the passed `Square` is not valid or `PieceType`
    /// is `PieceType::None`.
    pub fn place(&mut self, sq: Square, player: Player, piece_ty: PieceType) {
        debug_assert!(sq.is_okay());
        debug_assert_ne!(piece_ty, PieceType::None);
        self.arr[sq.0 as usize] = Piece::make(player, piece_ty);
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

pub struct BoardIterator<'a> {
    board: &'a Board,
    square: Option<Square>,
}

impl<'a> BoardIterator<'a> {
    pub fn new(board: &'a Board) -> Self {
        Self {
            board,
            square: Some(Square::A1),
        }
    }
}

impl<'a> Iterator for BoardIterator<'a> {
    type Item = (Square, Piece);

    fn next(&mut self) -> Option<Self::Item> {
        match self.square {
            Some(Square::H8) => {
                self.square = None;
                Some((Square::H8, self.board.piece_at_sq(Square::H8)))
            }
            Some(sq) => {
                self.square = Some(sq + Square(1));
                Some((sq, self.board.piece_at_sq(sq)))
            }
            None => None,
        }
    }
}

impl<'a> IntoIterator for &'a Board {
    type Item = (Square, Piece);
    type IntoIter = BoardIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        BoardIterator::new(self)
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_string())
    }
}
