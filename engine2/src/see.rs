//! Implementation of Static Exchange Evaluation.

use super::eval::piece_value;
use super::score::Score;
use super::search::Search;

use core::bb::Bitboard;
use core::position::{PieceType, Player, Square};

use std::cmp::max;

impl Search {
    /// The SEE swap algorithm.
    ///
    /// Returns the statically minimaxed outcome of exchanges on square `to` after current player
    /// captures a piece of type `target` with a piece of type `attacker` on square `from`. This
    /// analysis includes the effect of x-rays by sliding pieces through friendly pieces which
    /// move earlier (e.g. rook batteries along a file).
    pub fn see(
        &mut self,
        from: Square,
        to: Square,
        target: PieceType,
        mut attacker: PieceType,
    ) -> Score {
        let mut gain: [Score; 32] = [Score::cp(0); 32];
        let mut d: usize = 0;

        let may_xray = self.pos.piece_bb_both_players(PieceType::Pawn)
            | self.pos.piece_bb_both_players(PieceType::Bishop)
            | self.pos.piece_bb_both_players(PieceType::Rook)
            | self.pos.piece_bb_both_players(PieceType::Queen);
        let mut from_set = from.to_bb();
        let mut occ = self.pos.occupied();
        let mut atta_def = self.pos.attack_defend(occ, to);
        let mut side = self.pos.turn().other_player();

        // Need to track the processed sliding pieces to ensure that we don't repeatedly process
        // them in x-ray attacks.
        let mut processed = Bitboard::empty();

        gain[0] = Score::cp(piece_value(target));

        while !from_set.is_empty() {
            d += 1;

            gain[d] = Score::cp(piece_value(attacker)) - gain[d - 1];

            if max(-gain[d - 1], gain[d]) < Score::cp(0) {
                break;
            }

            atta_def ^= from_set;
            occ ^= from_set;
            processed ^= from_set;

            if !(from_set & may_xray).is_empty() {
                atta_def |= self.pos.attack_defend_sliding(occ, from_set.to_square()) & !processed;
            }

            (attacker, from_set) = self.least_valuable_piece(atta_def, side);

            side = side.other_player();
        }

        d -= 1;

        while d > 0 {
            gain[d - 1] = -max(-gain[d - 1], gain[d]);
            d -= 1;
        }

        gain[0]
    }

    fn least_valuable_piece(&self, atta_def: Bitboard, side: Player) -> (PieceType, Bitboard) {
        const PIECES: [PieceType; 6] = [
            PieceType::Pawn,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
            PieceType::King,
        ];

        for piece_type in PIECES {
            let subset = atta_def & self.pos.piece_bb(side, piece_type);

            if !subset.is_empty() {
                return (piece_type, subset.lsb());
            }
        }

        return (PieceType::None, Bitboard::empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::position::Position;

    #[test]
    fn it_works() {
        core::init::init_globals();

        let suite = #[rustfmt::skip] {
            vec![
                ("1k1r4/1pp4p/p7/4p3/8/P5P1/1PP4P/2K1R3 w - - 0 1", Square::E1, Square::E5, PieceType::Pawn, PieceType::Rook, Score::cp(100)),
                ("1k1r3q/1ppn3p/p4b2/4p3/8/P2N2P1/1PP1R1BP/2K1Q3 w - - 0 1", Square::D3, Square::E5, PieceType::Pawn, PieceType::Knight, Score::cp(-200)),
                ("k3q3/4r1n1/4p3/8/8/4R3/4Q3/K3R3 w - - 0 1", Square::E3, Square::E6, PieceType::Pawn, PieceType::Rook, Score::cp(-400)),
                ("k3q3/4r3/4p3/8/8/4R3/4R3/K3Q3 w - - 0 1", Square::E3, Square::E6, PieceType::Pawn, PieceType::Rook, Score::cp(100)),
                ("k3q3/4r3/4p3/8/8/8/4R3/K3Q3 w - - 0 1", Square::E2, Square::E6, PieceType::Pawn, PieceType::Rook, Score::cp(-400)),
                ("k3nrn1/4b3/3q1p1R/8/4N1NB/2Q5/5R2/K7 w - - 0 1", Square::E4, Square::F6, PieceType::Pawn, PieceType::Knight, Score::cp(100)),
                ("k3nrn1/4b3/3q1p1R/8/4N1NB/2Q5/5R2/K7 w - - 0 1", Square::C3, Square::F6, PieceType::Pawn, PieceType::Queen, Score::cp(-800)),
                ("k4r2/8/5q2/6P1/4N3/8/8/K7 w - - 0 1", Square::G5, Square::F6, PieceType::Queen, PieceType::Pawn, Score::cp(900)),
                ("k4r2/8/5q2/6P1/4N3/8/8/K7 w - - 0 1", Square::E4, Square::F6, PieceType::Queen, PieceType::Knight, Score::cp(900)),
                ("k7/3n4/8/4n3/2N5/5N2/8/K7 w - - 0 1", Square::C4, Square::E5, PieceType::Knight, PieceType::Knight, Score::cp(300)),
                ("k7/8/3n4/5N2/8/8/8/K4R2 b - - 0 1", Square::D6, Square::F5, PieceType::Knight, PieceType::Knight, Score::cp(0)),
                ("k7/8/3n4/5N2/8/8/8/K7 b - - 0 1", Square::D6, Square::F5, PieceType::Knight, PieceType::Knight, Score::cp(300)),
                ("k4r2/8/8/5N2/8/8/8/K7 b - - 0 1", Square::F8, Square::F5, PieceType::Knight, PieceType::Rook, Score::cp(300)),
                ("k4r2/8/8/5N2/8/6N1/8/K7 b - - 0 1", Square::F8, Square::F5, PieceType::Knight, PieceType::Rook, Score::cp(-200)),
                ("k6q/6b1/5b2/4B3/8/2B5/1B6/K7 b - - 0 1", Square::F6, Square::E5, PieceType::Bishop, PieceType::Bishop, Score::cp(0)),
                ("k7/8/2B2n2/8/4Q3/8/3n1N2/K7 b - - 0 1", Square::F6, Square::E4, PieceType::Queen, PieceType::Knight, Score::cp(900)),
                ("k7/8/8/3p1p2/4P3/3P1P2/8/K7 b - - 0 1", Square::D5, Square::E4, PieceType::Pawn, PieceType::Pawn, Score::cp(0)),
                ("k7/7b/8/3p1p2/4P3/3P1P2/8/K7 b - - 0 1", Square::D5, Square::E4, PieceType::Pawn, PieceType::Pawn, Score::cp(100)),
                ("k7/7b/8/5p2/4PK2/8/5N2/8 b - - 0 1", Square::F5, Square::E4, PieceType::Pawn, PieceType::Pawn, Score::cp(0)),
                ("8/1b6/3k4/3p4/3KP3/8/6B1/8 w - - 0 1", Square::E4, Square::D5, PieceType::Pawn, PieceType::Pawn, Score::cp(100)),

                // TODO: need to reflect the positions and discussion here. Pawns promoting with
                // capture, or pawns promoting without capture as the first move of SEE need
                // attention. It might be easiest to use a search extension whenever we have a pawn
                // on the 7th?
                // http://www.talkchess.com/forum3/viewtopic.php?f=7&t=77787

                // In these examples, the answer returned is not the true result because of
                // pruning. The result is nevertheless the same (in terms of whether the initial
                // capture is deemed favourable).
                ("k7/8/2B2n2/8/4Q3/5P2/3n4/K7 b - - 0 1", Square::F6, Square::E4, PieceType::Queen, PieceType::Knight, Score::cp(900)),
                ("k7/8/3np3/5R2/8/3Q2N1/8/K4R2 b - - 0 1", Square::E6, Square::F5, PieceType::Rook, PieceType::Pawn, Score::cp(500)),
            ]
        };

        for (fen, from, to, target, attacker, score) in suite {
            let pos = Position::from_fen(fen).unwrap();
            let mut search = Search::new(pos, Default::default());
            let see = search.see(from, to, target, attacker);
            assert_eq!(see, score);
        }
    }
}
