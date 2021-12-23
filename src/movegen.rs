use crate::bb::{
    Bitboard, SECOND_RANK, THIRD_RANK, WHITE_LEFT_PAWN_CAPTURE_MASK, WHITE_RIGHT_PAWN_CAPTURE_MASK,
};
use crate::mov::Move;
use crate::position::{Color, Position};

impl Position {
    pub fn generate_moves(&self) -> Vec<Move> {
        self.generate_pawn_moves()
    }

    fn generate_pawn_moves(&self) -> Vec<Move> {
        if self.turn == Color::White {
            let white_single_pawn_moves = self.white_pawns << 8 & self.no_piece;
            let white_double_pawn_moves =
                (white_single_pawn_moves & THIRD_RANK) << 8 & self.no_piece;

            let white_left_pawn_captures =
                (self.white_pawns << 7) & self.black_pieces & WHITE_LEFT_PAWN_CAPTURE_MASK;

            let white_right_pawn_captures =
                (self.white_pawns << 9) & self.black_pieces & WHITE_RIGHT_PAWN_CAPTURE_MASK;
        }
        Vec::new()
    }
}
