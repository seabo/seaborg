use core::position::{PieceType, Player, Position};

pub const PAWN_VALUE: i16 = 100;
pub const KNIGHT_VALUE: i16 = 300;
pub const BISHOP_VALUE: i16 = 300;
pub const ROOK_VALUE: i16 = 500;
pub const QUEEN_VALUE: i16 = 900;
pub const KING_VALUE: i16 = 10000;

pub const PIECE_VALUES: [i16; 7] = [
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
    fn material_eval(&self) -> i16;
}

impl Evaluation for Position {
    fn material_eval(&self) -> i16 {
        material_evaluation(self)
    }
}

fn material_evaluation(pos: &Position) -> i16 {
    pos.piece_bb(Player::WHITE, PieceType::Pawn).popcnt() as i16 * PAWN_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Knight).popcnt() as i16 * KNIGHT_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Bishop).popcnt() as i16 * BISHOP_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Rook).popcnt() as i16 * ROOK_VALUE
        + pos.piece_bb(Player::WHITE, PieceType::Queen).popcnt() as i16 * QUEEN_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Pawn).popcnt() as i16 * PAWN_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Knight).popcnt() as i16 * KNIGHT_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Bishop).popcnt() as i16 * BISHOP_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Rook).popcnt() as i16 * ROOK_VALUE
        - pos.piece_bb(Player::BLACK, PieceType::Queen).popcnt() as i16 * QUEEN_VALUE
}

/// The material evaluation of `PieceType`.
pub fn piece_value(piece_type: PieceType) -> i16 {
    unsafe { *PIECE_VALUES.get_unchecked(piece_type as usize) }
}
