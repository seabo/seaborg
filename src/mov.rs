use crate::position::{PieceType, Square};

#[derive(Copy, Clone, Debug)]
pub enum SpecialMove {
    None,
    Promotion,
    EnPassant,
    Castling,
}

#[derive(Copy, Clone, Debug)]
pub struct Move {
    orig: Square,
    dest: Square,
    promo_piece_type: Option<PieceType>,
    special_move: SpecialMove,
}

impl Move {
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
