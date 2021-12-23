use crate::bb::{
    Bitboard, FIFTH_RANK, SECOND_RANK, SEVENTH_RANK, THIRD_RANK, WHITE_LEFTWARD_PROMOTION_MASK,
    WHITE_LEFT_PAWN_CAPTURE_MASK, WHITE_RIGHTWARD_PROMOTION_MASK, WHITE_RIGHT_PAWN_CAPTURE_MASK,
    WHITE_SINGLE_PAWN_MOVE_MASK,
};
use crate::mov::Move;
use crate::position::{Player, Position, Square, PROMO_PIECES};
use crate::precalc::boards::{king_moves, knight_moves};

impl Position {
    pub fn generate_moves(&self) -> Vec<Move> {
        let mut move_list = Vec::new();

        if self.turn() == Player::White {
            self.generate_white_pawn_moves(&mut move_list);
            self.generate_white_king_moves(&mut move_list);
            self.generate_white_knight_moves(&mut move_list);
        }

        move_list
    }

    fn generate_white_pawn_moves(&self, move_list: &mut Vec<Move>) {
        let white_single_pawn_moves =
            (self.white_pawns & WHITE_SINGLE_PAWN_MOVE_MASK) << 8 & self.no_piece;

        for dest in white_single_pawn_moves {
            move_list.push(Move::build(
                Square(dest - 8),
                Square(dest),
                None,
                false,
                false,
            ));
        }

        let white_double_pawn_moves = (white_single_pawn_moves & THIRD_RANK) << 8 & self.no_piece;

        for dest in white_double_pawn_moves {
            move_list.push(Move::build(
                Square(dest - 16),
                Square(dest),
                None,
                false,
                false,
            ));
        }

        let white_left_pawn_captures =
            (self.white_pawns << 7) & self.black_pieces & WHITE_LEFT_PAWN_CAPTURE_MASK;

        for dest in white_left_pawn_captures {
            move_list.push(Move::build(
                Square(dest - 7),
                Square(dest),
                None,
                false,
                false,
            ));
        }

        let white_right_pawn_captures =
            (self.white_pawns << 9) & self.black_pieces & WHITE_RIGHT_PAWN_CAPTURE_MASK;

        for dest in white_right_pawn_captures {
            move_list.push(Move::build(
                Square(dest - 9),
                Square(dest),
                None,
                false,
                false,
            ));
        }

        let white_forward_promotions = (self.white_pawns & SEVENTH_RANK) << 8 & self.no_piece;

        for dest in white_forward_promotions {
            for promo_piece in PROMO_PIECES {
                move_list.push(Move::build(
                    Square(dest - 8),
                    Square(dest),
                    Some(promo_piece),
                    false,
                    false,
                ));
            }
        }

        let white_leftward_promotions =
            (self.white_pawns & WHITE_LEFTWARD_PROMOTION_MASK) << 7 & self.black_pieces;

        for dest in white_leftward_promotions {
            for promo_piece in PROMO_PIECES {
                move_list.push(Move::build(
                    Square(dest - 7),
                    Square(dest),
                    Some(promo_piece),
                    false,
                    false,
                ));
            }
        }

        let white_rightward_promotions =
            (self.white_pawns & WHITE_RIGHTWARD_PROMOTION_MASK) << 9 & self.black_pieces;

        for dest in white_rightward_promotions {
            for promo_piece in PROMO_PIECES {
                move_list.push(Move::build(
                    Square(dest - 9),
                    Square(dest),
                    Some(promo_piece),
                    false,
                    false,
                ));
            }
        }

        if let Some(Square(ep_square)) = self.ep_square {
            let ep = Bitboard::from_sq_idx(ep_square);
            let white_fifth_rank_pawns = self.white_pawns & FIFTH_RANK;
            let white_captures =
                ((ep >> 7) & white_fifth_rank_pawns) | ((ep >> 9) & white_fifth_rank_pawns);
            for orig in white_captures {
                move_list.push(Move::build(
                    Square(orig),
                    Square(ep_square),
                    None,
                    true,
                    false,
                ))
            }
        }
    }

    fn generate_white_king_moves(&self, move_list: &mut Vec<Move>) {
        let sq = Square(self.white_king.bsf() as u8);
        let king_moves = king_moves(sq) & !self.white_pieces;

        for dest in king_moves {
            move_list.push(Move::build(sq, Square(dest), None, false, false));
        }
    }

    fn generate_white_knight_moves(&self, move_list: &mut Vec<Move>) {
        for orig in self.white_knights {
            let knight_moves = knight_moves(Square(orig)) & !self.white_pieces;
            for dest in knight_moves {
                move_list.push(Move::build(Square(orig), Square(dest), None, false, false));
            }
        }
    }
}
