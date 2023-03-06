use core::position::{PieceType, Player, Position};

pub const PAWN_VALUE: i32 = 100;
pub const KNIGHT_VALUE: i32 = 300;
pub const BISHOP_VALUE: i32 = 300;
pub const ROOK_VALUE: i32 = 500;
pub const QUEEN_VALUE: i32 = 900;
pub const KING_VALUE: i32 = 10000;

pub const PIECE_VALUES: [i32; 7] = [
    0, // PieceType::None,
    PAWN_VALUE,
    KNIGHT_VALUE,
    BISHOP_VALUE,
    ROOK_VALUE,
    QUEEN_VALUE,
    KING_VALUE,
];

/// Adds static evaluation functionality to a type representing a chess position.
pub trait Evaluation {
    /// Simple material evaluation
    fn material_eval(&self) -> i32;
}

impl Evaluation for Position {
    fn material_eval(&self) -> i32 {
        material_evaluation(self)
    }
}

fn material_evaluation(pos: &Position) -> i32 {
    pos.piece_bb(Player::WHITE, PieceType::Pawn).popcnt() as i32 * PAWN_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Knight).popcnt() as i32 * KNIGHT_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Bishop).popcnt() as i32 * BISHOP_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Rook).popcnt() as i32 * ROOK_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Queen).popcnt() as i32 * QUEEN_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Pawn).popcnt() as i32 * PAWN_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Knight).popcnt() as i32 * KNIGHT_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Bishop).popcnt() as i32 * BISHOP_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Rook).popcnt() as i32 * ROOK_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Queen).popcnt() as i32 * QUEEN_VALUE
}

/// The material evaluation of `PieceType`.
pub fn piece_value(piece_type: PieceType) -> i32 {
    unsafe { *PIECE_VALUES.get_unchecked(piece_type as usize) }
}
