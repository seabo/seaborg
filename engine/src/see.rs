//! Implementation of Static Exchange Evaluation.

use super::eval::piece_value;
use super::score::Score;
use super::search::Search;

use core::bb::Bitboard;
use core::position::{PieceType, Player, Square};

use std::cmp::max;

impl<'engine> Search<'engine> {
    /// The SEE swap algorithm.
    ///
    /// Returns the statically minimaxed outcome of exchanges on square `to` after current player
    /// captures a piece of type `target` with a piece of type `attacker` on square `from`. This
    /// analysis includes the effect of x-rays by sliding pieces through friendly pieces which
    /// move earlier (e.g. rook batteries along a file).
    ///
    /// A pawn arriving on its back rank is treated as promoting to a queen: the move gains the
    /// difference in material, and the piece it leaves on `to` for the opponent to capture is a
    /// queen rather than a pawn. This covers both a capture that promotes and a plain push to the
    /// back rank opening the sequence; for the latter, pass `PieceType::None` as `target`.
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
        // The player who moves next in the sequence, i.e. the one who recaptures on `to`.
        let mut side = self.pos.turn().other_player();

        // Need to track the processed sliding pieces to ensure that we don't repeatedly process
        // them in x-ray attacks.
        let mut processed = Bitboard::empty();

        let promotion_gain =
            Score::cp(piece_value(PieceType::Queen) - piece_value(PieceType::Pawn));
        let mut promoting = promotes(attacker, to, self.pos.turn());

        gain[0] = Score::cp(piece_value(target)) + promotion_bonus(promoting, promotion_gain);

        while !from_set.is_empty() {
            d += 1;

            // The piece this move leaves on `to` for the opponent to take. A promoting pawn is
            // captured back as a queen.
            let standing_promoted = promoting;
            let standing = if standing_promoted {
                Score::cp(piece_value(PieceType::Queen))
            } else {
                Score::cp(piece_value(attacker))
            };

            // Vacate the origin square before picking the next attacker, so that x-rays through it
            // are revealed in time to be considered. Clear rather than toggle: a pawn pushing to
            // the back rank never attacked `to`, so toggling would insert a stale attacker and let
            // the same pawn be selected again two plies later.
            atta_def &= !from_set;
            occ ^= from_set;
            processed ^= from_set;

            if !(from_set & may_xray).is_empty() {
                let from = from_set
                    .to_square()
                    .expect("least valuable attacker must be a single square");
                atta_def |= self.pos.attack_defend_sliding(occ, from) & !processed;
            }

            (attacker, from_set) = self.least_valuable_piece(atta_def, side);
            promoting = promotes(attacker, to, side);

            // `gain[d]` is the payoff of the *next* move, so it has to be scored after that move is
            // known: it wins whatever is standing on `to`, plus its own promotion gain, minus the
            // opponent's standing gain. When no attacker remains this entry is speculative and the
            // minimax pass below discards it.
            gain[d] = standing + promotion_bonus(promoting, promotion_gain) - gain[d - 1];

            // The usual cutoff assumes that continuing the exchange cannot recover from both
            // alternatives being negative. Promotion breaks that assumption by making the
            // standing piece substantially more valuable than its pawn attacker, so retain the
            // immediate recapture and let the minimax pass account for it.
            if !standing_promoted && max(-gain[d - 1], gain[d]) < Score::cp(0) {
                break;
            }

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

        (PieceType::None, Bitboard::empty())
    }
}

/// Whether `mover` playing `piece` to `to` promotes.
///
/// Underpromotion is ignored: a promoting pawn is always valued as a queen.
#[inline(always)]
fn promotes(piece: PieceType, to: Square, mover: Player) -> bool {
    piece == PieceType::Pawn && to.rank() == mover.relative_rank(7)
}

#[inline(always)]
fn promotion_bonus(promoting: bool, promotion_gain: Score) -> Score {
    if promoting {
        promotion_gain
    } else {
        Score::cp(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::position::Position;
    use std::sync::atomic::AtomicBool;

    #[test]
    #[rustfmt::skip]
    fn it_works() {
        core::init::init_globals();

        let suite = vec![
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

                // Promotions. A pawn reaching the back rank is scored as a queen appearing on
                // `to`, so the exchange picks up the queen/pawn difference.
                // http://www.talkchess.com/forum3/viewtopic.php?f=7&t=77787

                // The reference position from that thread. Rxc8 looks like it merely trades into
                // Bxc8, but bxc8=Q follows, so black declines the recapture and white keeps the
                // rook.
                ("2r5/1P4pk/p2p1b1p/5b1n/BB3p2/2R2p2/P1P2P2/4RK2 w - - 0 1", Square::C3, Square::C8, PieceType::Rook, PieceType::Rook, Score::cp(500)),

                // A capture that promotes, as the first move of the sequence. Nothing recaptures,
                // so the gain is the rook plus the queen/pawn difference.
                ("2r5/1P6/8/8/8/6k1/8/6K1 w - - 0 1", Square::B7, Square::C8, PieceType::Rook, PieceType::Pawn, Score::cp(1300)),

                // A non-capturing promotion opening the sequence: pass `PieceType::None` as the
                // target. e8=Q, Rxe8, Rxe8 nets the queen/pawn difference less the exchange.
                ("r7/4P3/8/6k1/8/8/8/4R1K1 w - - 0 1", Square::E7, Square::E8, PieceType::None, PieceType::Pawn, Score::cp(400)),

                // A capture-promotion followed by a recapture. The promotion-aware cutoff must
                // retain ...Rxc8 so minimax reports the true net material gain.
                ("2rr4/1P6/8/8/8/6k1/8/6K1 w - - 0 1", Square::B7, Square::C8, PieceType::Rook, PieceType::Pawn, Score::cp(400)),

                // In these examples, the answer returned is not the true result because of
                // pruning. The result is nevertheless the same (in terms of whether the initial
                // capture is deemed favourable).
                ("k7/8/2B2n2/8/4Q3/5P2/3n4/K7 b - - 0 1", Square::F6, Square::E4, PieceType::Queen, PieceType::Knight, Score::cp(900)),
                ("k7/8/3np3/5R2/8/3Q2N1/8/K4R2 b - - 0 1", Square::E6, Square::F5, PieceType::Rook, PieceType::Pawn, Score::cp(500)),
        ];

        for (fen, from, to, target, attacker, score) in suite {
            let pos = Position::from_fen(fen).unwrap();
            let flag = AtomicBool::new(false);
            let tt = crate::tt::Table::new(1);
            let mut search = Search::new(pos, &flag, None, &tt);
            let see = search.see(from, to, target, attacker);
            assert_eq!(see, score);
        }
    }
}
