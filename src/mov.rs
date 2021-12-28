use crate::position::{PieceType, Square};
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SpecialMove {
    None,
    Promotion,
    EnPassant,
    Castling,
    Capture,
    Quiet,
    Null,
}

#[derive(Copy, Clone, Debug)]
pub struct Move {
    orig: Square,
    dest: Square,
    promo_piece_type: Option<PieceType>,
    special_move: SpecialMove,
}

impl Move {
    /// Build a null move. Used for initialising MoveList arrays
    pub fn null() -> Self {
        Self {
            orig: Square(64),
            dest: Square(64),
            promo_piece_type: None,
            special_move: SpecialMove::Null,
        }
    }

    pub fn is_null(&self) -> bool {
        self.special_move == SpecialMove::Null
    }

    /// Builds a move from an origin square, destination square
    /// and information about special moves like promotion, en
    /// passant and castling.
    ///
    /// Note: if you pass a promotion piece and `true` for en
    /// passant or castling, there will be undefined behaviour.
    /// To save time, this method assumes the data passed is
    /// already consistent and does no checks.
    pub fn build(
        orig: Square,
        dest: Square,
        promo_piece_type: Option<PieceType>,
        is_ep: bool,
        is_castling: bool,
    ) -> Self {
        let mut special_move = SpecialMove::None;

        if let Some(_) = promo_piece_type {
            special_move = SpecialMove::Promotion;
        } else if is_ep {
            special_move = SpecialMove::EnPassant;
        } else if is_castling {
            special_move = SpecialMove::Castling;
        }

        Self {
            orig,
            dest,
            promo_piece_type,
            special_move,
        }
    }

    pub fn dest(&self) -> Square {
        self.dest
    }

    pub fn orig(&self) -> Square {
        self.orig
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(promo_piece) = self.promo_piece_type {
            write!(f, "{}{}{:1}", self.orig, self.dest, promo_piece)
        } else {
            write!(f, "{}{}", self.orig, self.dest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    /// Ensure that the Move storage struct doesn't accidentally get bigger
    /// than 4 bytes.
    #[test]
    fn move_is_four_bytes() {
        assert_eq!(mem::size_of::<Move>(), 4);
    }
}
