use crate::position::{CastlingRights, Piece, Position, Square};
use crate::precalc::zobrist::{
    castling_rights_keys, ep_file_keys, piece_square_key, side_to_move_key, side_to_move_toggler,
};

use std::fmt;
use std::ops::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Zobrist(pub u64);

impl_bit_ops!(Zobrist, u64);

impl Zobrist {
    pub fn empty() -> Self {
        Zobrist(0)
    }

    /// Generates a `Zobrist` key from an otherwise fully built
    /// `Position` struct.
    pub fn from_position(pos: &Position) -> Self {
        let mut zob = Zobrist::empty();
        // Piece-squares
        for (sq, piece) in &pos.board {
            zob ^= piece_square_key(piece, sq);
        }
        // Side-to-move
        zob ^= side_to_move_key(pos.turn());
        // Castling rights
        zob ^= castling_rights_keys(pos.castling_rights());
        // Ep square
        match pos.ep_square() {
            Some(sq) => zob ^= ep_file_keys(sq),
            None => {}
        };

        zob
    }

    /// Updates a Zobrist key by xor'ing with the piece-square key for the given `Piece` and `Square`.
    /// For normal moves, this will be called twice: once to remove the key for where the piece started,
    /// and once to add in the key for where the piece moves to. For a capture, there will be another call,
    /// to toggle off the key for the captured piece.
    pub fn toggle_piece_sq(&mut self, piece: Piece, sq: Square) {
        *self ^= piece_square_key(piece, sq);
    }

    /// Called exactly once for each move.
    pub fn toggle_side_to_move(&mut self) {
        *self ^= side_to_move_toggler();
    }

    /// Checks if old and new castling rights differ, and if so it toggles both the old key and new key.
    pub fn update_castling_rights(&mut self, old: CastlingRights, new: CastlingRights) {
        if old != new {
            *self ^= castling_rights_keys(old);
            *self ^= castling_rights_keys(new);
        }
    }

    /// Update a Zobrist key from an old en passant square to a new one.
    pub fn update_ep_square(&mut self, old: Option<Square>, new: Option<Square>) {
        match old {
            Some(sq) => *self ^= ep_file_keys(sq),
            None => {}
        }

        match new {
            Some(sq) => *self ^= ep_file_keys(sq),
            None => {}
        }
    }
}

impl fmt::Display for Zobrist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:b}", self.0)
    }
}
