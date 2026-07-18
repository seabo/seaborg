//! Formatting typed game and search state for the browser transport.
//!
//! This is the browser counterpart to [`crate::info`], which formats the same typed values for
//! UCI. Both adapters read the engine's typed reports; neither is authoritative over game state.

use super::json::{write_key, write_string};
use crate::game::{CommandError, DrawReason, EngineStatus, GameSnapshot, GameStatus, MoveRecord};
use crate::score::Score;
use crate::search::SearchProgress;
use core::position::Player;
use std::fmt::Write as _;

fn player_name(player: Player) -> &'static str {
    if player.is_white() {
        "white"
    } else {
        "black"
    }
}

/// Parse a side name from a browser command.
pub fn parse_player(name: &str) -> Option<Player> {
    match name {
        "white" => Some(Player::WHITE),
        "black" => Some(Player::BLACK),
        _ => None,
    }
}

/// The stable machine-readable code for a rejected command.
///
/// The browser branches on these, so they are part of the protocol contract rather than prose.
pub fn command_error_code(error: &CommandError) -> &'static str {
    match error {
        CommandError::StaleRevision { .. } => "stale_revision",
        CommandError::NotHumanTurn => "not_human_turn",
        CommandError::GameOver => "game_over",
        CommandError::IllegalMove => "illegal_move",
        CommandError::NothingToUndo => "nothing_to_undo",
    }
}

fn write_move_record(out: &mut String, record: &MoveRecord) {
    let mut first = true;
    out.push('{');
    write_key(out, &mut first, "uci");
    write_string(out, &record.uci);
    write_key(out, &mut first, "san");
    write_string(out, &record.san);
    out.push('}');
}

fn write_game_status(out: &mut String, status: GameStatus) {
    let mut first = true;
    out.push('{');
    match status {
        GameStatus::Ongoing => {
            write_key(out, &mut first, "kind");
            write_string(out, "ongoing");
        }
        GameStatus::Checkmate { winner } => {
            write_key(out, &mut first, "kind");
            write_string(out, "checkmate");
            write_key(out, &mut first, "winner");
            write_string(out, player_name(winner));
        }
        GameStatus::Draw(reason) => {
            write_key(out, &mut first, "kind");
            write_string(out, "draw");
            write_key(out, &mut first, "reason");
            write_string(
                out,
                match reason {
                    DrawReason::Stalemate => "stalemate",
                    DrawReason::ThreefoldRepetition => "threefold_repetition",
                    DrawReason::FiftyMoveRule => "fifty_move_rule",
                },
            );
        }
    }
    out.push('}');
}

/// Write a score as a tagged value so the browser never has to decode the engine's integer coding.
fn write_score(out: &mut String, score: Score) {
    let mut first = true;
    out.push('{');
    let raw = score.to_i16();
    if score == Score::INF_P || score == Score::INF_N {
        // The infinities sit outside the mate band but satisfy `is_mate`, so they must be taken
        // first — exactly as `Score`'s `Display` does. Falling through would run the conversion
        // below on a value it was never derived for and invert the sign: `INF_P` would render as
        // a mate *against* the side to move. No search result reaches the browser as an infinity
        // today, so this guards a representation rather than a live path.
        write_key(out, &mut first, "kind");
        write_string(out, if score == Score::INF_P { "inf" } else { "-inf" });
    } else if score.is_mate() {
        // Mirrors the UCI `mate N` conversion in `Score`'s `Display`: negative means the side to
        // move is being mated, and the value counts moves rather than plies.
        let moves = if raw < 0 {
            -((raw + 20_100) / 2)
        } else {
            (20_100 - raw + 1) / 2
        };
        write_key(out, &mut first, "kind");
        write_string(out, "mate");
        write_key(out, &mut first, "moves");
        let _ = write!(out, "{moves}");
    } else {
        write_key(out, &mut first, "kind");
        write_string(out, "cp");
        write_key(out, &mut first, "centipawns");
        let _ = write!(out, "{raw}");
    }
    out.push('}');
}

fn write_progress(out: &mut String, progress: &SearchProgress) {
    let mut first = true;
    out.push('{');
    write_key(out, &mut first, "depth");
    let _ = write!(out, "{}", progress.depth);
    write_key(out, &mut first, "score");
    write_score(out, progress.score);
    write_key(out, &mut first, "elapsedMs");
    let _ = write!(out, "{}", progress.elapsed.as_millis());
    write_key(out, &mut first, "nodes");
    let _ = write!(out, "{}", progress.nodes);
    write_key(out, &mut first, "nps");
    let _ = write!(out, "{}", progress.nps);
    write_key(out, &mut first, "hashfull");
    let _ = write!(out, "{}", progress.hashfull);
    write_key(out, &mut first, "principalVariation");
    out.push('[');
    for (index, mov) in progress.principal_variation.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_string(out, &mov.to_uci_string());
    }
    out.push(']');
    out.push('}');
}

fn write_engine_status(out: &mut String, status: &EngineStatus) {
    let mut first = true;
    out.push('{');
    match status {
        EngineStatus::Idle => {
            write_key(out, &mut first, "kind");
            write_string(out, "idle");
        }
        EngineStatus::Thinking {
            search_id,
            position_revision,
            progress,
        } => {
            write_key(out, &mut first, "kind");
            write_string(out, "thinking");
            write_key(out, &mut first, "searchId");
            let _ = write!(out, "{search_id}");
            write_key(out, &mut first, "positionRevision");
            let _ = write!(out, "{position_revision}");
            write_key(out, &mut first, "progress");
            match progress {
                Some(progress) => write_progress(out, progress),
                None => out.push_str("null"),
            }
        }
    }
    out.push('}');
}

/// Serialize a complete authoritative snapshot for the browser.
pub fn snapshot_to_json(snapshot: &GameSnapshot) -> String {
    let mut out = String::with_capacity(1024);
    let mut first = true;
    out.push('{');

    write_key(&mut out, &mut first, "revision");
    let _ = write!(out, "{}", snapshot.revision);
    write_key(&mut out, &mut first, "humanSide");
    write_string(&mut out, player_name(snapshot.human_side));
    write_key(&mut out, &mut first, "fen");
    write_string(&mut out, &snapshot.fen);
    write_key(&mut out, &mut first, "sideToMove");
    write_string(&mut out, player_name(snapshot.side_to_move));
    write_key(&mut out, &mut first, "inCheck");
    out.push_str(if snapshot.in_check { "true" } else { "false" });

    write_key(&mut out, &mut first, "legalMoves");
    out.push('[');
    for (index, uci) in snapshot.legal_moves.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_string(&mut out, uci);
    }
    out.push(']');

    write_key(&mut out, &mut first, "lastMove");
    match &snapshot.last_move {
        Some(record) => write_move_record(&mut out, record),
        None => out.push_str("null"),
    }

    write_key(&mut out, &mut first, "moveHistory");
    out.push('[');
    for (index, record) in snapshot.move_history.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_move_record(&mut out, record);
    }
    out.push(']');

    write_key(&mut out, &mut first, "gameStatus");
    write_game_status(&mut out, snapshot.game_status);
    write_key(&mut out, &mut first, "engineStatus");
    write_engine_status(&mut out, &snapshot.engine_status);

    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::json::{parse, Json};
    use core::init::init_globals;
    use core::position::Position;
    use std::time::Duration;

    fn start_snapshot() -> GameSnapshot {
        init_globals();
        let position = Position::start_pos();
        GameSnapshot {
            revision: 3,
            human_side: Player::WHITE,
            fen: position.to_fen(),
            side_to_move: Player::WHITE,
            in_check: false,
            legal_moves: vec!["e2e4".to_owned(), "d2d4".to_owned()],
            last_move: None,
            move_history: Vec::new(),
            game_status: GameStatus::Ongoing,
            engine_status: EngineStatus::Idle,
        }
    }

    #[test]
    fn serializes_a_snapshot_the_parser_accepts() {
        let mut snapshot = start_snapshot();
        snapshot.in_check = true;
        let json = snapshot_to_json(&snapshot);
        let value = parse(&json).unwrap();
        assert_eq!(value.get("revision").unwrap().as_u64(), Some(3));
        assert_eq!(value.get("humanSide").unwrap().as_str(), Some("white"));
        assert_eq!(value.get("sideToMove").unwrap().as_str(), Some("white"));
        assert_eq!(value.get("inCheck"), Some(&Json::Bool(true)));
        assert_eq!(value.get("lastMove"), Some(&Json::Null));
        assert_eq!(
            value
                .get("gameStatus")
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str(),
            Some("ongoing")
        );
        assert_eq!(
            value
                .get("engineStatus")
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str(),
            Some("idle")
        );
        let moves = match value.get("legalMoves").unwrap() {
            Json::Array(items) => items,
            other => panic!("expected array, got {other:?}"),
        };
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].as_str(), Some("e2e4"));
    }

    #[test]
    fn serializes_terminal_and_draw_statuses() {
        let mut snapshot = start_snapshot();
        snapshot.game_status = GameStatus::Checkmate {
            winner: Player::BLACK,
        };
        let value = parse(&snapshot_to_json(&snapshot)).unwrap();
        let status = value.get("gameStatus").unwrap();
        assert_eq!(status.get("kind").unwrap().as_str(), Some("checkmate"));
        assert_eq!(status.get("winner").unwrap().as_str(), Some("black"));

        for (reason, expected) in [
            (DrawReason::Stalemate, "stalemate"),
            (DrawReason::ThreefoldRepetition, "threefold_repetition"),
            (DrawReason::FiftyMoveRule, "fifty_move_rule"),
        ] {
            snapshot.game_status = GameStatus::Draw(reason);
            let value = parse(&snapshot_to_json(&snapshot)).unwrap();
            let status = value.get("gameStatus").unwrap();
            assert_eq!(status.get("kind").unwrap().as_str(), Some("draw"));
            assert_eq!(status.get("reason").unwrap().as_str(), Some(expected));
        }
    }

    #[test]
    fn serializes_history_and_thinking_progress() {
        init_globals();
        let mut position = Position::start_pos();
        let mov = position.make_uci_move("e2e4").unwrap();
        let mut snapshot = start_snapshot();
        snapshot.last_move = Some(MoveRecord {
            uci: "e2e4".to_owned(),
            san: "e4".to_owned(),
        });
        snapshot.move_history = vec![snapshot.last_move.clone().unwrap()];
        snapshot.engine_status = EngineStatus::Thinking {
            search_id: 7,
            position_revision: 3,
            progress: Some(SearchProgress {
                depth: 5,
                score: Score::cp(-42),
                elapsed: Duration::from_millis(250),
                nodes: 9_000,
                nps: 36_000,
                hashfull: 11,
                principal_variation: vec![mov],
            }),
        };

        let value = parse(&snapshot_to_json(&snapshot)).unwrap();
        assert_eq!(
            value.get("lastMove").unwrap().get("san").unwrap().as_str(),
            Some("e4")
        );
        let engine = value.get("engineStatus").unwrap();
        assert_eq!(engine.get("kind").unwrap().as_str(), Some("thinking"));
        assert_eq!(engine.get("searchId").unwrap().as_u64(), Some(7));
        assert_eq!(engine.get("positionRevision").unwrap().as_u64(), Some(3));
        let progress = engine.get("progress").unwrap();
        assert_eq!(progress.get("depth").unwrap().as_u64(), Some(5));
        assert_eq!(progress.get("elapsedMs").unwrap().as_u64(), Some(250));
        assert_eq!(progress.get("nodes").unwrap().as_u64(), Some(9_000));
        let score = progress.get("score").unwrap();
        assert_eq!(score.get("kind").unwrap().as_str(), Some("cp"));
        assert_eq!(score.get("centipawns"), Some(&Json::Number(-42.0)));
        let pv = match progress.get("principalVariation").unwrap() {
            Json::Array(items) => items,
            other => panic!("expected array, got {other:?}"),
        };
        assert_eq!(pv[0].as_str(), Some("e2e4"));
    }

    #[test]
    fn reports_thinking_without_progress_as_null() {
        let mut snapshot = start_snapshot();
        snapshot.engine_status = EngineStatus::Thinking {
            search_id: 1,
            position_revision: 0,
            progress: None,
        };
        let value = parse(&snapshot_to_json(&snapshot)).unwrap();
        assert_eq!(
            value.get("engineStatus").unwrap().get("progress"),
            Some(&Json::Null)
        );
    }

    #[test]
    fn encodes_mate_scores_in_moves_matching_uci() {
        for moves in [1_i8, 3, 5] {
            let mut out = String::new();
            write_score(&mut out, Score::mate(2 * moves - 1));
            let value = parse(&out).unwrap();
            assert_eq!(value.get("kind").unwrap().as_str(), Some("mate"));
            assert_eq!(
                value.get("moves"),
                Some(&Json::Number(f64::from(moves))),
                "positive mate in {moves}"
            );

            let mut out = String::new();
            write_score(&mut out, Score::mate(-2 * moves));
            let value = parse(&out).unwrap();
            assert_eq!(value.get("kind").unwrap().as_str(), Some("mate"));
            assert_eq!(
                value.get("moves"),
                Some(&Json::Number(f64::from(-moves))),
                "negative mate in {moves}"
            );
        }
    }

    #[test]
    fn maps_side_names_and_command_error_codes() {
        assert_eq!(parse_player("white"), Some(Player::WHITE));
        assert_eq!(parse_player("black"), Some(Player::BLACK));
        assert_eq!(parse_player("White"), None);
        assert_eq!(parse_player(""), None);

        assert_eq!(
            command_error_code(&CommandError::StaleRevision {
                expected: 1,
                received: 0
            }),
            "stale_revision"
        );
        assert_eq!(
            command_error_code(&CommandError::IllegalMove),
            "illegal_move"
        );
        assert_eq!(command_error_code(&CommandError::GameOver), "game_over");
        assert_eq!(
            command_error_code(&CommandError::NothingToUndo),
            "nothing_to_undo"
        );
        assert_eq!(
            command_error_code(&CommandError::NotHumanTurn),
            "not_human_turn"
        );
    }

    /// Review attempt 1: `write_score` tested `is_mate` before the infinities, which satisfy it
    /// while sitting outside the mate band. `INF_P` therefore ran through the positive-mate
    /// conversion and came out as `{"kind":"mate","moves":-4949}` — an infinite advantage
    /// rendered as being mated. `Score`'s `Display` takes the infinities first; so does this now.
    #[test]
    fn infinite_scores_are_tagged_rather_than_converted_as_mates() {
        let mut out = String::new();
        write_score(&mut out, Score::INF_P);
        assert_eq!(out, r#"{"kind":"inf"}"#);

        let mut out = String::new();
        write_score(&mut out, Score::INF_N);
        assert_eq!(out, r#"{"kind":"-inf"}"#);

        // The guard is exact, so the mate band it sits above is untouched — the deepest mate this
        // engine represents still converts. `encodes_mate_scores_in_moves_matching_uci` covers
        // the band itself; this only shows the new branch does not swallow part of it.
        let mut out = String::new();
        write_score(&mut out, Score::mate(-98));
        assert_eq!(out, r#"{"kind":"mate","moves":-49}"#);
    }
}
