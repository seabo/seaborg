use crate::position::{PieceType, Square};
use std::fmt;

/// Following the Stockfish scheme:
///
/// A move needs 16 bits to be stored
///
/// bit  0- 5: destination square (from 0 to 63)
/// bit  6-11: origin square (from 0 to 63)
/// bit 12-13: promotion piece type - 2 (from KNIGHT-2 to QUEEN-2)
/// bit 14-15: special move flag: promotion (1), en passant (2), castling (3)
/// NOTE: en passant bit is set only when a pawn can be captured
///
/// Special cases are None and Null. We can sneak these in because in
/// any normal move destination square is always different from origin square
/// while MOVE_NONE and MOVE_NULL have the same origin and destination square.
#[derive(Copy, Clone)]
pub struct Move(u16);

impl Move {
    pub fn build(
        orig: Square,
        dest: Square,
        promotion_type: Option<PieceType>,
        is_ep: bool,
        is_castling: bool,
    ) -> Self {
        let mut m: u16 = 0;

        m ^= orig as u16;
        m ^= (dest as u16) << 6;

        if let Some(promo_piece) = promotion_type {
            let promo_piece_flag: u16 = match promo_piece {
                PieceType::Knight => 0,
                PieceType::Bishop => 1,
                PieceType::Rook => 2,
                PieceType::Queen => 3,
                _ => panic!(
                    "cannot build a move where the promotion piece is a {}",
                    promo_piece
                ),
            };

            m ^= promo_piece_flag << 12;
            m ^= 1 << 14;
            m ^= 1 << 13;
        }

        if is_ep {
            m ^= 1 << 15;
        }

        if is_castling {
            m ^= 3 << 14;
        }

        Self(m)
    }

    pub fn dest(&self) -> Square {
        let dest = self.0 >> 6 & 0x2F;
        Square::from_idx(dest)
    }
}

pub struct MoveStruct {
    orig: Square,
    dest: Square,
    promo_piece_type: Option<PieceType>,
    special_move_flag: u8,
}

impl MoveStruct {
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

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let orig = self.0 & 0x3F;
        let dest = (self.0 & 0xFC0) >> 6;
        let promo_piece_flag = (self.0 & 0x3000) >> 12;
        let special_move_flag = (self.0 & 0xC000) >> 14;
        let promo_piece = if special_move_flag == 1 {
            match promo_piece_flag {
                0 => PieceType::Knight,
                1 => PieceType::Bishop,
                2 => PieceType::Rook,
                3 => PieceType::Queen,
                _ => panic!(
                    "cannot have a move where the promotion piece is a {}",
                    promo_piece_flag
                ),
            }
        } else {
            PieceType::None
        };

        write!(f, "{:02b} ", special_move_flag)?;
        write!(f, "{:02b} ", promo_piece_flag)?;
        write!(f, "{:06b} ", orig)?;
        write!(f, "{:06b}\n", dest)?;

        write!(f, "-- -- ------ ------\n")?;
        write!(
            f,
            "{:1} {:1}    {}     {}  \n",
            "sp",
            promo_piece,
            Square::from_idx(orig),
            Square::from_idx(dest)
        )
    }
}
