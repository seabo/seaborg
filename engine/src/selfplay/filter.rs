//! Position filtering: which self-play positions are worth keeping as training
//! samples.
//!
//! A self-play game produces one scored position per ply, but not all of them
//! make good training data. A position whose side to move is in check, or whose
//! best move is a capture, is tactically unsettled: its static evaluation is
//! about to be overturned by a forcing move, so the label is noisy for a network
//! that scores quiet positions. The earliest plies of a game are also close to
//! book and over-represented across games. This module drops those categories
//! and keeps the rest.
//!
//! Filtering is a per-game operation because one of its rules is positional: the
//! "early opening plies" cut needs to know how far into the game a position sits,
//! which is its index within the game, not anything stored on the position.

use super::{GameRecord, Sample};

/// Configurable rules for deciding which positions a game contributes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PositionFilter {
    /// Drop positions whose side to move is in check. A check forces the reply,
    /// so the quiet evaluation the network learns does not describe the position.
    pub skip_in_check: bool,
    /// Drop positions whose search best move is a capture. The evaluation before
    /// a capture reflects a material swing that is about to happen, not the
    /// settled position, so it teaches the network the wrong target.
    pub skip_best_move_capture: bool,
    /// Drop positions at a ply strictly below this within their game. The opening
    /// plies are near-book and repeat across games; excluding them keeps the data
    /// from over-weighting them. Zero keeps every ply.
    pub skip_opening_plies: usize,
}

impl Default for PositionFilter {
    fn default() -> Self {
        Self {
            skip_in_check: true,
            skip_best_move_capture: true,
            skip_opening_plies: 0,
        }
    }
}

impl PositionFilter {
    /// Whether `sample`, sitting at `ply` within its game (0-based), is kept.
    pub fn retains(&self, sample: &Sample, ply: usize) -> bool {
        if ply < self.skip_opening_plies {
            return false;
        }
        if self.skip_in_check && sample.position.in_check() {
            return false;
        }
        if self.skip_best_move_capture && sample.best_move.is_some_and(|mov| mov.is_capture()) {
            return false;
        }
        true
    }

    /// The samples of `record` this filter keeps, in game order. The ply each
    /// rule sees is the sample's index within the game.
    pub fn retained<'a>(&'a self, record: &'a GameRecord) -> impl Iterator<Item = &'a Sample> + 'a {
        record
            .samples
            .iter()
            .enumerate()
            .filter(move |(ply, sample)| self.retains(sample, *ply))
            .map(|(_, sample)| sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Score;
    use crate::selfplay::{GameResult, Termination, Wdl};
    use chess::init::init_globals;
    use chess::mono_traits::{All, Legal};
    use chess::mov::Move;
    use chess::movelist::BasicMoveList;
    use chess::position::{Player, Position};

    fn position(fen: &str) -> Position {
        init_globals();
        Position::from_fen(fen).expect("valid FEN")
    }

    /// A capture move that is legal in `position`, for use as a sample's best
    /// move. Panics if the position has no capture, keeping the test honest.
    fn a_capture(position: &Position) -> Move {
        position
            .generate::<BasicMoveList, All, Legal>()
            .iter()
            .find(|mov| mov.is_capture())
            .copied()
            .expect("position was expected to have a capture")
    }

    fn a_quiet(position: &Position) -> Move {
        position
            .generate::<BasicMoveList, All, Legal>()
            .iter()
            .find(|mov| !mov.is_capture())
            .copied()
            .expect("position was expected to have a quiet move")
    }

    fn sample(position: Position, best_move: Option<Move>) -> Sample {
        Sample {
            position,
            score: Score::zero(),
            outcome: Wdl::Draw,
            best_move,
        }
    }

    #[test]
    fn in_check_positions_are_dropped_when_enabled() {
        // White king on e1 is checked by a rook on e8.
        let checked = position("4r3/8/8/8/8/8/8/4K2k w - - 0 1");
        let filter = PositionFilter {
            skip_in_check: true,
            skip_best_move_capture: false,
            skip_opening_plies: 0,
        };
        assert!(!filter.retains(&sample(checked.clone(), None), 20));
        // The same position is kept once the in-check rule is switched off.
        let permissive = PositionFilter {
            skip_in_check: false,
            ..filter
        };
        assert!(permissive.retains(&sample(checked, None), 20));
    }

    #[test]
    fn capture_best_moves_are_dropped_when_enabled() {
        // White to move can take the undefended black rook on d5.
        let tactical = position("7k/8/8/3r4/4P3/8/8/7K w - - 0 1");
        let capture = a_capture(&tactical);
        assert!(capture.is_capture());
        let filter = PositionFilter {
            skip_in_check: false,
            skip_best_move_capture: true,
            skip_opening_plies: 0,
        };
        assert!(!filter.retains(&sample(tactical.clone(), Some(capture)), 20));
        // A quiet best move in the same position is kept.
        let quiet = a_quiet(&tactical);
        assert!(filter.retains(&sample(tactical, Some(quiet)), 20));
    }

    #[test]
    fn early_plies_are_dropped_up_to_the_threshold() {
        let quiet = position("7k/8/8/8/8/8/8/7K w - - 0 1");
        let filter = PositionFilter {
            skip_in_check: false,
            skip_best_move_capture: false,
            skip_opening_plies: 6,
        };
        // Plies 0..6 are excluded; ply 6 and beyond are kept.
        assert!(!filter.retains(&sample(quiet.clone(), None), 0));
        assert!(!filter.retains(&sample(quiet.clone(), None), 5));
        assert!(filter.retains(&sample(quiet.clone(), None), 6));
        assert!(filter.retains(&sample(quiet, None), 7));
    }

    #[test]
    fn retained_reports_the_ply_of_each_sample() {
        // Three samples: ply 0 quiet (dropped by the opening cut), ply 1 in
        // check (dropped by the check rule), ply 2 quiet (kept).
        let quiet = position("7k/8/8/8/8/8/8/7K w - - 0 1");
        let checked = position("4r3/8/8/8/8/8/8/4K2k w - - 0 1");
        let record = GameRecord {
            samples: vec![
                sample(quiet.clone(), None),
                sample(checked, None),
                sample(quiet.clone(), None),
            ],
            result: GameResult::Draw,
            termination: Termination::MaxPlies,
        };
        let filter = PositionFilter {
            skip_in_check: true,
            skip_best_move_capture: false,
            skip_opening_plies: 1,
        };
        let kept: Vec<&Sample> = filter.retained(&record).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].position, quiet);
        assert_eq!(kept[0].position.turn(), Player::WHITE);
    }
}
