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
    ///
    /// This must agree exactly with the incremental updates that `make_move_unchecked` applies,
    /// because the two derivations meet in the transposition table: a search rooted at a parsed
    /// position keys its entries one way and a search that reached the same position by playing
    /// moves keys them the other. Empty squares are therefore skipped. Iterating the board yields
    /// all 64 squares, including empty ones as `Piece::None`, and folding in a key for those made
    /// the full recomputation disagree with the incremental path by the `Piece::None` keys of every
    /// square whose occupancy changed, splitting one position across two identities.
    pub fn from_position(pos: &Position) -> Self {
        let mut zob = Zobrist::empty();
        // Piece-squares
        for (sq, piece) in &pos.board {
            if piece != Piece::None {
                zob ^= piece_square_key(piece, sq);
            }
        }
        // Side-to-move
        zob ^= side_to_move_key(pos.turn());
        // Castling rights
        zob ^= castling_rights_keys(pos.castling_rights());
        // Ep square
        if let Some(sq) = pos.ep_square() {
            zob ^= ep_file_keys(sq)
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
        if let Some(sq) = old {
            *self ^= ep_file_keys(sq)
        }

        if let Some(sq) = new {
            *self ^= ep_file_keys(sq)
        }
    }
}

impl fmt::Display for Zobrist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for Zobrist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for Zobrist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::Binary for Zobrist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Binary::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono_traits::{All, Legal};
    use crate::movelist::BasicMoveList;

    /// The incremental key maintained by `make_move_unchecked` and the full key computed by
    /// `from_position` must never disagree, or the same position occupies two transposition-table
    /// identities depending on how the search reached it.
    #[test]
    fn incremental_and_full_keys_agree_after_every_legal_move() {
        crate::init::init_globals();

        let fens = [
            // Quiet moves, double pushes and an available en-passant capture.
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
            // Captures, castling and promotions, which move or remove more than one piece.
            "r3k2r/pPpp1ppp/8/8/8/8/PpPP1PPP/R3K2R w KQkq - 0 1",
        ];

        for fen in fens {
            let pos = Position::from_fen(fen).unwrap();
            assert_eq!(
                pos.zobrist(),
                Zobrist::from_position(&pos),
                "parsed position disagreed with its own recomputation: {fen}"
            );

            for mov in pos.generate::<BasicMoveList, All, Legal>().iter() {
                let mut after = pos.clone();
                after.make_move(mov);

                assert_eq!(
                    after.zobrist(),
                    Zobrist::from_position(&after),
                    "incremental and full keys disagreed after {mov} from {fen}"
                );
            }
        }
    }

    /// Unmaking must restore the key exactly, so a search returns to the identity it descended from.
    #[test]
    fn unmaking_a_move_restores_the_key() {
        crate::init::init_globals();

        let mut pos =
            Position::from_fen("r3k2r/pPpp1ppp/8/8/8/8/PpPP1PPP/R3K2R w KQkq - 0 1").unwrap();
        let before = pos.zobrist();

        for mov in pos.clone().generate::<BasicMoveList, All, Legal>().iter() {
            pos.make_move(mov);
            pos.unmake_move();
            assert_eq!(
                pos.zobrist(),
                before,
                "key not restored after unmaking {mov}"
            );
        }
    }
}
