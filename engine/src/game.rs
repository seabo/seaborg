//! Transport-independent ownership of a human-versus-engine game.

use crate::search::{SearchEngine, SearchHandle, SearchLimit, SearchOutcome, SearchProgress};
use core::mono_traits::{All, Legal};
use core::mov::Move;
use core::movelist::BasicMoveList;
use core::position::{PieceType, Player, Position};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawReason {
    Stalemate,
    ThreefoldRepetition,
    FiftyMoveRule,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameStatus {
    Ongoing,
    Checkmate { winner: Player },
    Draw(DrawReason),
}

impl GameStatus {
    pub fn is_terminal(self) -> bool {
        self != Self::Ongoing
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveRecord {
    pub uci: String,
    pub san: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineStatus {
    Idle,
    Thinking {
        search_id: u64,
        position_revision: u64,
        progress: Option<SearchProgress>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSnapshot {
    pub revision: u64,
    pub human_side: Player,
    pub fen: String,
    pub side_to_move: Player,
    pub legal_moves: Vec<String>,
    pub last_move: Option<MoveRecord>,
    pub move_history: Vec<MoveRecord>,
    pub game_status: GameStatus,
    pub engine_status: EngineStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandError {
    StaleRevision { expected: u64, received: u64 },
    NotHumanTurn,
    GameOver,
    IllegalMove,
    NothingToUndo,
}

struct ActiveSearch {
    id: u64,
    revision: u64,
    progress: Option<SearchProgress>,
    handle: SearchHandle,
}

/// The single owner of all mutable state for one game session.
pub struct GameController {
    position: Position,
    human_side: Player,
    revision: u64,
    next_search_id: u64,
    history: Vec<MoveRecord>,
    status: GameStatus,
    search_limit: SearchLimit,
    search_engine: SearchEngine,
    active_search: Option<ActiveSearch>,
}

impl GameController {
    pub fn new(human_side: Player, search_limit: SearchLimit, hash_size_mb: usize) -> Self {
        Self::from_position(
            Position::start_pos(),
            human_side,
            search_limit,
            hash_size_mb,
        )
    }

    pub fn from_position(
        position: Position,
        human_side: Player,
        search_limit: SearchLimit,
        hash_size_mb: usize,
    ) -> Self {
        let status = position_status(&position);
        let mut controller = Self {
            position,
            human_side,
            revision: 0,
            next_search_id: 1,
            history: Vec::new(),
            status,
            search_limit,
            search_engine: SearchEngine::new(hash_size_mb),
            active_search: None,
        };
        controller.start_engine_turn();
        controller
    }

    pub fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            revision: self.revision,
            human_side: self.human_side,
            fen: self.position.to_fen(),
            side_to_move: self.position.turn(),
            legal_moves: legal_uci_moves(&self.position),
            last_move: self.history.last().cloned(),
            move_history: self.history.clone(),
            game_status: self.status,
            engine_status: self
                .active_search
                .as_ref()
                .map_or(EngineStatus::Idle, |search| EngineStatus::Thinking {
                    search_id: search.id,
                    position_revision: search.revision,
                    progress: search.progress.clone(),
                }),
        }
    }

    pub fn play_human_move(&mut self, uci: &str, revision: u64) -> Result<(), CommandError> {
        if revision != self.revision {
            return Err(CommandError::StaleRevision {
                expected: self.revision,
                received: revision,
            });
        }
        if self.status.is_terminal() {
            return Err(CommandError::GameOver);
        }
        if self.position.turn() != self.human_side || self.active_search.is_some() {
            return Err(CommandError::NotHumanTurn);
        }

        let mov = find_uci_move(&self.position, uci).ok_or(CommandError::IllegalMove)?;
        self.apply_move(mov);
        self.start_engine_turn();
        Ok(())
    }

    /// Consume available search events and apply a current, completed best move.
    /// Returns true when the published snapshot changed.
    pub fn poll(&mut self) -> bool {
        let Some(mut active) = self.active_search.take() else {
            return false;
        };
        let mut changed = false;
        for event in active.handle.events().try_iter() {
            if let crate::search::SearchEvent::Progress(progress) = event {
                active.progress = Some(progress);
                changed = true;
            }
        }
        if !active.handle.is_finished() {
            self.active_search = Some(active);
            return changed;
        }

        let id = active.id;
        let revision = active.revision;
        let outcome = active.handle.wait();
        changed = true;
        if let SearchOutcome::Completed(Some(result)) = outcome {
            let Some(best_move) = result.best_move else {
                return changed;
            };
            if revision == self.revision
                && self.position.turn() != self.human_side
                && self.status == GameStatus::Ongoing
                && find_uci_move(&self.position, &best_move.to_uci_string()) == Some(best_move)
                && id < self.next_search_id
            {
                self.apply_move(best_move);
            }
        }
        changed
    }

    pub fn reset(&mut self, human_side: Player) {
        self.replace_position(Position::start_pos(), human_side);
    }

    pub fn reset_to(&mut self, position: Position, human_side: Player) {
        self.replace_position(position, human_side);
    }

    /// Undo the last full turn, stopping once it is the human's turn again.
    pub fn undo(&mut self, revision: u64) -> Result<(), CommandError> {
        if revision != self.revision {
            return Err(CommandError::StaleRevision {
                expected: self.revision,
                received: revision,
            });
        }
        if self.position.unmake_move().is_none() {
            return Err(CommandError::NothingToUndo);
        }
        self.cancel_search();
        self.history.pop();
        while self.position.turn() != self.human_side && self.position.unmake_move().is_some() {
            self.history.pop();
        }
        self.revision += 1;
        self.status = position_status(&self.position);
        self.start_engine_turn();
        Ok(())
    }

    fn replace_position(&mut self, position: Position, human_side: Player) {
        self.cancel_search();
        self.position = position;
        self.human_side = human_side;
        self.history.clear();
        self.revision += 1;
        self.status = position_status(&self.position);
        self.start_engine_turn();
    }

    fn cancel_search(&mut self) {
        if let Some(search) = self.active_search.take() {
            search.handle.cancel();
        }
    }

    fn apply_move(&mut self, mov: Move) {
        let uci = mov.to_uci_string();
        let san = move_to_san(&self.position, mov);
        self.position.make_move(&mov);
        self.history.push(MoveRecord { uci, san });
        self.revision += 1;
        self.status = position_status(&self.position);
    }

    fn start_engine_turn(&mut self) {
        if self.status.is_terminal()
            || self.position.turn() == self.human_side
            || self.active_search.is_some()
        {
            return;
        }
        let id = self.next_search_id;
        self.next_search_id += 1;
        self.active_search = Some(ActiveSearch {
            id,
            revision: self.revision,
            progress: None,
            handle: self
                .search_engine
                .start(self.position.clone(), self.search_limit),
        });
    }
}

fn legal_moves(position: &Position) -> BasicMoveList {
    position.generate::<BasicMoveList, All, Legal>()
}

fn legal_uci_moves(position: &Position) -> Vec<String> {
    legal_moves(position)
        .iter()
        .map(Move::to_uci_string)
        .collect()
}

fn find_uci_move(position: &Position, uci: &str) -> Option<Move> {
    legal_moves(position)
        .iter()
        .find(|mov| mov.to_uci_string() == uci)
        .copied()
}

fn position_status(position: &Position) -> GameStatus {
    let moves = legal_moves(position);
    if moves.is_empty() {
        if position.in_check() {
            GameStatus::Checkmate {
                winner: !position.turn(),
            }
        } else {
            GameStatus::Draw(DrawReason::Stalemate)
        }
    } else if position.in_threefold() {
        GameStatus::Draw(DrawReason::ThreefoldRepetition)
    } else if position.fifty_move_rule_reached() {
        GameStatus::Draw(DrawReason::FiftyMoveRule)
    } else {
        GameStatus::Ongoing
    }
}

pub fn move_to_san(position: &Position, mov: Move) -> String {
    let piece_type = position.piece_at_sq(mov.orig()).type_of();
    let mut san = String::new();
    if mov.is_castle() {
        san.push_str(if mov.dest() > mov.orig() {
            "O-O"
        } else {
            "O-O-O"
        });
    } else {
        if piece_type != PieceType::Pawn {
            san.push(match piece_type {
                PieceType::Knight => 'N',
                PieceType::Bishop => 'B',
                PieceType::Rook => 'R',
                PieceType::Queen => 'Q',
                PieceType::King => 'K',
                _ => unreachable!(),
            });
            let alternatives: Vec<Move> = legal_moves(position)
                .iter()
                .filter(|other| {
                    **other != mov
                        && other.dest() == mov.dest()
                        && position.piece_at_sq(other.orig()).type_of() == piece_type
                })
                .copied()
                .collect();
            if !alternatives.is_empty() {
                let same_file = alternatives
                    .iter()
                    .any(|other| other.orig().file() == mov.orig().file());
                let same_rank = alternatives
                    .iter()
                    .any(|other| other.orig().rank() == mov.orig().rank());
                if !same_file {
                    san.push((b'a' + mov.orig().file()) as char);
                } else if !same_rank {
                    san.push((b'1' + mov.orig().rank()) as char);
                } else {
                    san.push((b'a' + mov.orig().file()) as char);
                    san.push((b'1' + mov.orig().rank()) as char);
                }
            }
        } else if mov.is_capture() {
            san.push((b'a' + mov.orig().file()) as char);
        }
        if mov.is_capture() {
            san.push('x');
        }
        san.push_str(&mov.dest().to_string());
        if let Some(promotion) = mov.promo_piece_type() {
            san.push('=');
            san.push(match promotion {
                PieceType::Knight => 'N',
                PieceType::Bishop => 'B',
                PieceType::Rook => 'R',
                PieceType::Queen => 'Q',
                _ => unreachable!(),
            });
        }
    }

    let mut after = position.clone();
    after.make_move(&mov);
    if after.in_check() {
        san.push(if legal_moves(&after).is_empty() {
            '#'
        } else {
            '+'
        });
    }
    san
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::init::init_globals;
    use std::time::{Duration, Instant};

    fn controller(fen: &str, human: Player) -> GameController {
        init_globals();
        GameController::from_position(
            Position::from_fen(fen).unwrap(),
            human,
            SearchLimit::Depth(1),
            1,
        )
    }

    fn wait_for_engine(controller: &mut GameController) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.active_search.is_some() && Instant::now() < deadline {
            controller.poll();
            std::thread::yield_now();
        }
        assert!(controller.active_search.is_none());
    }

    #[test]
    fn snapshots_and_normal_play_are_authoritative() {
        let mut game = controller(core::position::START_POSITION, Player::WHITE);
        let initial = game.snapshot();
        assert_eq!(initial.revision, 0);
        assert_eq!(initial.legal_moves.len(), 20);
        game.play_human_move("e2e4", initial.revision).unwrap();
        assert_eq!(game.snapshot().last_move.unwrap().san, "e4");
        wait_for_engine(&mut game);
        assert_eq!(game.snapshot().move_history.len(), 2);
        assert_eq!(game.snapshot().side_to_move, Player::WHITE);
    }

    #[test]
    fn rejects_illegal_stale_and_wrong_side_commands() {
        let mut game = controller(core::position::START_POSITION, Player::WHITE);
        assert_eq!(
            game.play_human_move("e2e5", 0),
            Err(CommandError::IllegalMove)
        );
        game.play_human_move("e2e4", 0).unwrap();
        assert!(matches!(
            game.play_human_move("d2d4", 0),
            Err(CommandError::StaleRevision { .. })
        ));
        assert_eq!(
            game.play_human_move("e7e5", 1),
            Err(CommandError::NotHumanTurn)
        );

        let black = controller(core::position::START_POSITION, Player::BLACK);
        assert!(matches!(
            black.snapshot().engine_status,
            EngineStatus::Thinking { .. }
        ));
    }

    #[test]
    fn reset_and_undo_cancel_search_and_advance_revision() {
        let mut game = controller(core::position::START_POSITION, Player::WHITE);
        game.play_human_move("e2e4", 0).unwrap();
        let old_id = match game.snapshot().engine_status {
            EngineStatus::Thinking { search_id, .. } => search_id,
            _ => panic!("search was not started"),
        };
        game.undo(1).unwrap();
        assert_eq!(game.snapshot().revision, 2);
        assert_eq!(game.snapshot().fen, core::position::START_POSITION);
        assert_eq!(game.snapshot().engine_status, EngineStatus::Idle);

        game.reset(Player::BLACK);
        match game.snapshot().engine_status {
            EngineStatus::Thinking { search_id, .. } => assert!(search_id > old_id),
            _ => panic!("reset did not start the engine"),
        }
    }

    #[test]
    fn empty_undo_preserves_the_opening_engine_turn() {
        let mut game = controller(core::position::START_POSITION, Player::BLACK);
        let initial = game.snapshot();
        let initial_search_id = match initial.engine_status {
            EngineStatus::Thinking { search_id, .. } => search_id,
            _ => panic!("opening engine search was not started"),
        };

        assert_eq!(
            game.undo(initial.revision),
            Err(CommandError::NothingToUndo)
        );

        let after = game.snapshot();
        assert_eq!(after.revision, initial.revision);
        assert_eq!(after.fen, initial.fen);
        assert!(matches!(
            after.engine_status,
            EngineStatus::Thinking { search_id, .. } if search_id == initial_search_id
        ));
        wait_for_engine(&mut game);
        assert_eq!(game.snapshot().side_to_move, Player::BLACK);
    }

    #[test]
    fn stale_or_cancelled_search_outcomes_are_never_applied() {
        let mut game = controller(core::position::START_POSITION, Player::BLACK);
        let original = game.snapshot().fen;
        game.revision += 1;
        wait_for_engine(&mut game);
        assert_eq!(game.snapshot().fen, original);

        game.reset(Player::BLACK);
        game.cancel_search();
        assert_eq!(game.snapshot().fen, core::position::START_POSITION);
    }

    #[test]
    fn incomplete_search_outcomes_are_ignored() {
        init_globals();
        let mut game = GameController::new(Player::BLACK, SearchLimit::Time(Duration::ZERO), 1);
        let original = game.snapshot().fen;
        wait_for_engine(&mut game);
        assert_eq!(game.snapshot().fen, original);
        assert_eq!(game.snapshot().side_to_move, Player::WHITE);
        assert_eq!(game.snapshot().engine_status, EngineStatus::Idle);
    }

    #[test]
    fn san_covers_disambiguation_castling_and_en_passant() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/3N4/4K1N1 w - - 0 1").unwrap();
        assert_eq!(
            move_to_san(&pos, find_uci_move(&pos, "g1f3").unwrap()),
            "Ngf3"
        );

        let pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        assert_eq!(
            move_to_san(&pos, find_uci_move(&pos, "e1g1").unwrap()),
            "O-O"
        );
        assert_eq!(
            move_to_san(&pos, find_uci_move(&pos, "e1c1").unwrap()),
            "O-O-O"
        );

        let pos = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        assert_eq!(
            move_to_san(&pos, find_uci_move(&pos, "e5d6").unwrap()),
            "exd6"
        );
    }

    #[test]
    fn san_covers_promotion_check_and_checkmate() {
        let pos = Position::from_fen("7k/6P1/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(
            move_to_san(&pos, find_uci_move(&pos, "g7g8q").unwrap()),
            "g8=Q+"
        );

        let pos = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 w - - 0 1").unwrap();
        assert_eq!(
            move_to_san(&pos, find_uci_move(&pos, "f7g7").unwrap()),
            "Qg7#"
        );
    }

    #[test]
    fn detects_terminal_and_move_count_positions_without_searching() {
        let mate = controller("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1", Player::WHITE);
        assert_eq!(
            mate.snapshot().game_status,
            GameStatus::Checkmate {
                winner: Player::WHITE
            }
        );
        assert_eq!(mate.snapshot().engine_status, EngineStatus::Idle);

        let stalemate = controller("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", Player::WHITE);
        assert_eq!(
            stalemate.snapshot().game_status,
            GameStatus::Draw(DrawReason::Stalemate)
        );

        let fifty = controller("7k/8/8/8/8/8/8/K7 w - - 100 51", Player::WHITE);
        assert_eq!(
            fifty.snapshot().game_status,
            GameStatus::Draw(DrawReason::FiftyMoveRule)
        );
    }

    #[test]
    fn detects_threefold_repetition() {
        let mut game = controller(core::position::START_POSITION, Player::WHITE);
        for uci in [
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ] {
            let mov = find_uci_move(&game.position, uci).unwrap();
            game.apply_move(mov);
        }
        assert_eq!(
            game.status,
            GameStatus::Draw(DrawReason::ThreefoldRepetition)
        );
    }
}
