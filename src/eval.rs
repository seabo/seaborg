use crate::mono_traits::{
    BishopType, BlackType, KnightType, PawnType, PieceTrait, PlayerTrait, QueenType, RookType,
    WhiteType,
};
use crate::position::Position;

pub const PAWN_VALUE: i32 = 100;
pub const KNIGHT_VALUE: i32 = 300;
pub const BISHOP_VALUE: i32 = 300;
pub const ROOK_VALUE: i32 = 500;
pub const QUEEN_VALUE: i32 = 900;
pub const KING_VALUE: i32 = 10000;

pub fn material_eval(pos: &Position) -> i32 {
    material_eval_single_side::<WhiteType>(pos) - material_eval_single_side::<BlackType>(pos)
}

#[inline(always)]
pub fn material_eval_single_side<PL: PlayerTrait>(pos: &Position) -> i32 {
    material_eval_single_piece::<PL, PawnType>(pos)
        + material_eval_single_piece::<PL, KnightType>(pos)
        + material_eval_single_piece::<PL, BishopType>(pos)
        + material_eval_single_piece::<PL, RookType>(pos)
        + material_eval_single_piece::<PL, QueenType>(pos)
}

#[inline(always)]
pub fn material_eval_single_piece<PL: PlayerTrait, P: PieceTrait>(pos: &Position) -> i32 {
    pos.piece_bb(PL::player(), P::piece_type()).popcnt() as i32 * P::material_val()
}
