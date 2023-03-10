use crate::position::{CastlingRights, PieceType, Player, Position, Square, State};
use bitflags::bitflags;
use std::fmt;

bitflags! {
    pub struct MoveType: u8 {
        const PROMOTION  = 0b00000001;
        const EN_PASSANT = 0b00000010;
        const CASTLE     = 0b00000100;
        const CAPTURE    = 0b00001000;
        const QUIET      = 0b00010000;
        const NULL       = 0b00100000;
    }
}

/// Struct used to store moves which are generated in movegen.
///
/// This struct is always 4 bytes, and is deliberately kept relatively small.
///
/// There is not enough information in a `Move` to allow undoing. When a move
/// is actually made on the board, a `MoveHistory` struct is built which contains
/// more information allowing for efficient an `unmake_move()`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Move {
    orig: Square,
    dest: Square,
    promo_piece_type: Option<PieceType>,
    ty: MoveType,
}

impl Move {
    /// Build a null move. Used for initialising MoveList arrays
    pub fn null() -> Self {
        Self {
            orig: Square(64),
            dest: Square(64),
            promo_piece_type: None,
            ty: MoveType::NULL,
        }
    }

    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.ty.contains(MoveType::NULL)
    }

    #[inline(always)]
    pub fn is_capture(&self) -> bool {
        self.ty.contains(MoveType::CAPTURE)
    }

    #[inline(always)]
    pub fn is_en_passant(&self) -> bool {
        self.ty.contains(MoveType::EN_PASSANT)
    }

    #[inline(always)]
    pub fn is_castle(&self) -> bool {
        self.ty.contains(MoveType::CASTLE)
    }

    #[inline(always)]
    pub fn is_promo(&self) -> bool {
        debug_assert!(if self.ty.contains(MoveType::PROMOTION) {
            self.promo_piece_type.is_some()
        } else {
            self.promo_piece_type.is_none()
        });
        self.ty.contains(MoveType::PROMOTION)
    }

    #[inline(always)]
    pub fn promo_piece_type(&self) -> Option<PieceType> {
        self.promo_piece_type
    }

    /// Returns a clone of `self` with the promo piece type set to the passed value.
    #[inline(always)]
    pub fn set_promo_type(&self, promo_type: PieceType) -> Move {
        debug_assert!(self.is_promo());
        debug_assert!(&[
            PieceType::Queen,
            PieceType::Rook,
            PieceType::Bishop,
            PieceType::Knight
        ]
        .contains(&promo_type));

        let mut m = self.clone();
        m.promo_piece_type = Some(promo_type);
        m
    }

    /// Returns the type of move, according to the `SpecialMove` field.
    #[inline(always)]
    pub fn move_type(&self) -> MoveType {
        self.ty
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
        ty: MoveType,
    ) -> Self {
        Self {
            orig,
            dest,
            promo_piece_type,
            ty,
        }
    }

    #[inline(always)]
    pub fn dest(&self) -> Square {
        self.dest
    }

    #[inline(always)]
    pub fn orig(&self) -> Square {
        self.orig
    }

    pub fn to_undoable(&self, position: &Position) -> UndoableMove {
        let captured = if self.is_en_passant() {
            let us = position.turn();
            let cap_sq = match us {
                Player::WHITE => self.dest - Square(8),
                Player::BLACK => self.dest + Square(8),
            };
            position.piece_at_sq(cap_sq).type_of()
        } else {
            position.piece_at_sq(self.dest).type_of()
        };

        UndoableMove {
            orig: self.orig,
            dest: self.dest,
            promo_piece_type: self.promo_piece_type,
            captured,
            ty: self.ty,
            prev_castling_rights: position.castling_rights,
            prev_ep_square: position.ep_square,
            prev_half_move_clock: position.half_move_clock,
            state: position.state,
        }
    }

    /// Returns a string containing the uci encoding of this move.
    ///
    /// E.g. 'e2e4'
    pub fn to_uci_string(&self) -> String {
        if self.is_null() {
            return format!("Null");
        }

        if let Some(promo_piece) = self.promo_piece_type {
            format!("{}{}{:1}", self.orig, self.dest, promo_piece)
        } else {
            format!("{}{}", self.orig, self.dest)
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uci_string())
    }
}

/// A struct containing enough information to allow undoing a move on a
/// `Position`. This struct contains more data (like captured piece and
/// previous castling rights) than a basic `Move` struct. This is 16 bytes
/// in size, and is only used for undoing moves to save space.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct UndoableMove {
    pub orig: Square,
    pub dest: Square,
    pub promo_piece_type: Option<PieceType>,
    pub captured: PieceType,
    pub ty: MoveType,
    pub prev_castling_rights: CastlingRights,
    pub prev_ep_square: Option<Square>,
    pub prev_half_move_clock: u32,
    pub state: State,
}

impl UndoableMove {
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.ty.contains(MoveType::NULL)
    }

    #[inline(always)]
    pub fn is_en_passant(&self) -> bool {
        self.ty.contains(MoveType::EN_PASSANT)
    }

    #[inline(always)]
    pub fn is_castle(&self) -> bool {
        self.ty.contains(MoveType::CASTLE)
    }

    pub fn is_promo(&self) -> bool {
        debug_assert!(if self.ty.contains(MoveType::PROMOTION) {
            self.promo_piece_type.is_some()
        } else {
            self.promo_piece_type.is_none()
        });
        self.ty.contains(MoveType::PROMOTION)
    }

    /// Returns a string containing the uci encoding of this move.
    ///
    /// E.g. 'e2e4'
    pub fn to_uci_string(&self) -> String {
        if let Some(promo_piece) = self.promo_piece_type {
            format!("{}{}{:1}", self.orig, self.dest, promo_piece)
        } else {
            format!("{}{}", self.orig, self.dest)
        }
    }
}

impl fmt::Display for UndoableMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uci_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    /// Ensure that the Move storage struct doesn't accidentally get bigger
    /// than 4 bytes.
    #[test]
    fn move_is_4_bytes() {
        assert_eq!(mem::size_of::<Move>(), 4);
    }

    #[test]
    fn undoable_move_is_56_bytes() {
        assert_eq!(mem::size_of::<UndoableMove>(), 56);
    }
}
