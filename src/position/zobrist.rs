use crate::position::Position;
use crate::precalc::zobrist::piece_square_key;

use std::ops::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Zobrist(pub u64);

impl_bit_ops!(Zobrist, u64);

impl Zobrist {
    /// Generates a `Zobrist` key from an otherwise fully built
    /// `Position` struct.
    pub fn from_position(pos: &Position) -> Self {
        // Piece-squares
        // Side-to-move
        // Castling rights
        // Ep square
        let zob = Zobrist(1024);

        zob
    }
}
