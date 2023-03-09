use core::position::{PieceType, Position};
use core::{Bishop, Black, King, Knight, Pawn, PieceTrait, Queen, Rook, Side, White};

pub const PAWN_VALUE: i32 = 100;
pub const KNIGHT_VALUE: i32 = 300;
pub const BISHOP_VALUE: i32 = 300;
pub const ROOK_VALUE: i32 = 500;
pub const QUEEN_VALUE: i32 = 900;
pub const KING_VALUE: i32 = 10000;

fn material_eval(pos: &Position) -> i32 {
    material_eval_single_side::<White>(pos) - material_eval_single_side::<Black>(pos)
}

/// Provide static evaluation capabilities to a type representing a position.
pub trait Evaluation {
    /// Return the static evaluation of the current position.
    fn eval(&self) -> i32;
}

impl Evaluation for Position {
    fn eval(&self) -> i32 {
        material_eval(self)
    }
}

#[inline(always)]
pub fn material_eval_single_side<PL: Side>(pos: &Position) -> i32 {
    material_eval_single_piece::<PL, Pawn>(pos)
        + material_eval_single_piece::<PL, Knight>(pos)
        + material_eval_single_piece::<PL, Bishop>(pos)
        + material_eval_single_piece::<PL, Rook>(pos)
        + material_eval_single_piece::<PL, Queen>(pos)
}

#[inline(always)]
pub fn material_eval_single_piece<PL: Side, P: Material + PieceTrait>(pos: &Position) -> i32 {
    pos.piece_bb(PL::player(), P::kind()).popcnt() as i32 * P::material_val()
}

/// The `PieceTrait` allows for reusing movegen code by monomorphizing
/// over different piece types. This trait provides common functionality
/// across each piece type.
pub trait Material {
    /// Returns the material value for the `PieceType`.
    fn material_val() -> i32;
}

impl Material for Pawn {
    #[inline(always)]
    fn material_val() -> i32 {
        PAWN_VALUE
    }
}

impl Material for Knight {
    #[inline(always)]
    fn material_val() -> i32 {
        KNIGHT_VALUE
    }
}

impl Material for Bishop {
    #[inline(always)]
    fn material_val() -> i32 {
        BISHOP_VALUE
    }
}

impl Material for Rook {
    #[inline(always)]
    fn material_val() -> i32 {
        ROOK_VALUE
    }
}

impl Material for Queen {
    #[inline(always)]
    fn material_val() -> i32 {
        QUEEN_VALUE
    }
}

impl Material for King {
    #[inline(always)]
    fn material_val() -> i32 {
        KING_VALUE
    }
}

/// Trait to assign material values to the `PieceType` enum. Preferred to keep this
/// in the `engine` crate, rather than `core` which should only focus on chess rules.
pub trait Value {
    /// Material valuation.
    fn value(&self) -> i32;
}

impl Value for PieceType {
    fn value(&self) -> i32 {
        match self {
            PieceType::Pawn => PAWN_VALUE,
            PieceType::Knight => KNIGHT_VALUE,
            PieceType::Bishop => BISHOP_VALUE,
            PieceType::Rook => ROOK_VALUE,
            PieceType::Queen => QUEEN_VALUE,
            // We should never call `value()` on something which could be a king,
            // so have a panic to alert to a bug.
            PieceType::King => KING_VALUE,
            PieceType::None => 0,
        }
    }
}
