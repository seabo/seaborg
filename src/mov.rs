use crate::position::{PieceType, Square};
use std::fmt;

#[derive(Copy, Clone, Debug)]
pub struct Move {
    orig: Square,
    dest: Square,
    promo_piece_type: Option<PieceType>,
    // Todo: use an enum again
    special_move_flag: u8,
}

impl Move {
    pub fn build(
        orig: Square,
        dest: Square,
        promo_piece_type: Option<PieceType>,
        is_ep: bool,
        is_castling: bool,
    ) -> Self {
        let mut flags: u8 = 0;
        if let Some(_) = promo_piece_type {
            flags ^= 1;
        }

        if is_ep {
            flags = 2;
        }

        if is_castling {
            flags = 3;
        }

        Self {
            orig,
            dest,
            promo_piece_type,
            special_move_flag: flags,
        }
    }

    pub fn dest(&self) -> Square {
        self.dest
    }

    pub fn orig(&self) -> Square {
        self.orig
    }
}
