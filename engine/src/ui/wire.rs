//! Formatting typed game and search state for the browser transport.
//!
//! This is the browser counterpart to [`crate::info`], which formats the same typed values for
//! UCI. Both adapters read the engine's typed reports; neither is authoritative over game state.
//!
//! The wire shape is defined by the `*Dto` types below: serde serializes them in field-declaration
//! order, so the browser sees a fixed layout without this module hand-writing any JSON. The DTOs
//! borrow from the engine's typed values rather than owning a second copy of the state.

use crate::game::{CommandError, DrawReason, EngineStatus, GameSnapshot, GameStatus, MoveRecord};
use crate::score::Score;
use crate::search::{SearchLimit, SearchProgress};
use core::position::Player;
use serde::Serialize;
use std::time::Duration;

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

/// The shortest engine thinking time the browser may select, in milliseconds.
///
/// A limit below this leaves the guaranteed-minimum search doing all the work, so every choice
/// under it would play identically while reading as a distinct setting.
pub const MIN_ENGINE_TIME_MS: u64 = 50;

/// The longest engine thinking time the browser may select, in milliseconds.
///
/// The board is locked while the engine thinks, so an unbounded value entered by hand would look
/// exactly like the UI having hung.
pub const MAX_ENGINE_TIME_MS: u64 = 60_000;

/// The deepest fixed-depth engine limit the browser may select.
///
/// Fixed depth has no time bound at all, so this caps how long one turn can take. It is the depth
/// beyond which a mid-game search stops being interactive.
pub const MAX_ENGINE_DEPTH: u64 = 12;

/// Parse an engine limit from a browser command.
///
/// `Infinite` is deliberately unreachable: a search that only ends when cancelled would leave the
/// game with no engine reply and the board locked forever.
pub fn parse_engine_limit(kind: &str, value: u64) -> Result<SearchLimit, &'static str> {
    match kind {
        "time" => {
            if !(MIN_ENGINE_TIME_MS..=MAX_ENGINE_TIME_MS).contains(&value) {
                return Err("invalid_engine_limit");
            }
            Ok(SearchLimit::Time(Duration::from_millis(value)))
        }
        "depth" => {
            if !(1..=MAX_ENGINE_DEPTH).contains(&value) {
                return Err("invalid_engine_limit");
            }
            // The cap keeps this inside `u8`.
            Ok(SearchLimit::Depth(value as u8))
        }
        _ => Err("invalid_engine_limit"),
    }
}

/// The limit the next engine turn will use, tagged so the browser need not decode a unit.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum EngineLimitDto {
    Time { milliseconds: u128 },
    Depth { plies: u8 },
    Nodes { nodes: u64 },
    Infinite,
}

impl From<SearchLimit> for EngineLimitDto {
    fn from(limit: SearchLimit) -> Self {
        match limit {
            SearchLimit::Time(duration) => EngineLimitDto::Time {
                milliseconds: duration.as_millis(),
            },
            SearchLimit::Depth(plies) => EngineLimitDto::Depth { plies },
            SearchLimit::Nodes(nodes) => EngineLimitDto::Nodes { nodes },
            // Not reachable through `parse_engine_limit`, but the CLI default could name it and a
            // snapshot must stay total.
            SearchLimit::Infinite => EngineLimitDto::Infinite,
        }
    }
}

#[derive(Serialize)]
struct MoveRecordDto<'a> {
    uci: &'a str,
    san: &'a str,
}

impl<'a> From<&'a MoveRecord> for MoveRecordDto<'a> {
    fn from(record: &'a MoveRecord) -> Self {
        MoveRecordDto {
            uci: &record.uci,
            san: &record.san,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum GameStatusDto {
    Ongoing,
    Checkmate { winner: &'static str },
    Draw { reason: &'static str },
}

impl From<GameStatus> for GameStatusDto {
    fn from(status: GameStatus) -> Self {
        match status {
            GameStatus::Ongoing => GameStatusDto::Ongoing,
            GameStatus::Checkmate { winner } => GameStatusDto::Checkmate {
                winner: player_name(winner),
            },
            GameStatus::Draw(reason) => GameStatusDto::Draw {
                reason: match reason {
                    DrawReason::Stalemate => "stalemate",
                    DrawReason::ThreefoldRepetition => "threefold_repetition",
                    DrawReason::FiftyMoveRule => "fifty_move_rule",
                },
            },
        }
    }
}

/// A score, tagged so the browser never has to decode the engine's integer coding.
#[derive(Serialize)]
#[serde(tag = "kind")]
enum ScoreDto {
    #[serde(rename = "inf")]
    Inf,
    #[serde(rename = "-inf")]
    NegInf,
    #[serde(rename = "mate")]
    Mate { moves: i16 },
    #[serde(rename = "cp")]
    Cp { centipawns: i16 },
}

impl From<Score> for ScoreDto {
    fn from(score: Score) -> Self {
        let raw = score.to_i16();
        if score == Score::INF_P {
            // The infinities sit outside the mate band but satisfy `is_mate`, so they must be
            // taken first — exactly as `Score`'s `Display` does. Falling through would run the
            // conversion below on a value it was never derived for and invert the sign: `INF_P`
            // would render as a mate *against* the side to move. No search result reaches the
            // browser as an infinity today, so this guards a representation rather than a live path.
            ScoreDto::Inf
        } else if score == Score::INF_N {
            ScoreDto::NegInf
        } else if score.is_mate() {
            // Mirrors the UCI `mate N` conversion in `Score`'s `Display`: negative means the side
            // to move is being mated, and the value counts moves rather than plies.
            let moves = if raw < 0 {
                -((raw + 20_100) / 2)
            } else {
                (20_100 - raw + 1) / 2
            };
            ScoreDto::Mate { moves }
        } else {
            ScoreDto::Cp { centipawns: raw }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressDto {
    depth: u8,
    score: ScoreDto,
    elapsed_ms: u128,
    nodes: usize,
    nps: u32,
    hashfull: u16,
    principal_variation: Vec<String>,
}

impl From<&SearchProgress> for ProgressDto {
    fn from(progress: &SearchProgress) -> Self {
        ProgressDto {
            depth: progress.depth,
            score: progress.score.into(),
            elapsed_ms: progress.elapsed.as_millis(),
            nodes: progress.nodes,
            nps: progress.nps,
            hashfull: progress.hashfull,
            principal_variation: progress
                .principal_variation
                .iter()
                .map(|mov| mov.to_uci_string())
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum EngineStatusDto<'a> {
    Idle,
    #[serde(rename_all = "camelCase")]
    Thinking {
        search_id: u64,
        position_revision: u64,
        progress: Option<ProgressDto>,
        // SAN sits beside `progress` rather than inside it because the search reports moves and
        // only the controller can read them against a position.
        principal_variation_san: &'a [String],
    },
}

impl<'a> From<&'a EngineStatus> for EngineStatusDto<'a> {
    fn from(status: &'a EngineStatus) -> Self {
        match status {
            EngineStatus::Idle => EngineStatusDto::Idle,
            EngineStatus::Thinking {
                search_id,
                position_revision,
                progress,
                principal_variation_san,
            } => EngineStatusDto::Thinking {
                search_id: *search_id,
                position_revision: *position_revision,
                progress: progress.as_ref().map(ProgressDto::from),
                principal_variation_san,
            },
        }
    }
}

/// The complete authoritative snapshot the browser renders.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotDto<'a> {
    revision: u64,
    human_side: &'static str,
    fen: &'a str,
    side_to_move: &'static str,
    in_check: bool,
    legal_moves: &'a [String],
    last_move: Option<MoveRecordDto<'a>>,
    move_history: Vec<MoveRecordDto<'a>>,
    game_status: GameStatusDto,
    engine_status: EngineStatusDto<'a>,
    engine_limit: EngineLimitDto,
}

impl<'a> From<&'a GameSnapshot> for SnapshotDto<'a> {
    fn from(snapshot: &'a GameSnapshot) -> Self {
        SnapshotDto {
            revision: snapshot.revision,
            human_side: player_name(snapshot.human_side),
            fen: &snapshot.fen,
            side_to_move: player_name(snapshot.side_to_move),
            in_check: snapshot.in_check,
            legal_moves: &snapshot.legal_moves,
            last_move: snapshot.last_move.as_ref().map(MoveRecordDto::from),
            move_history: snapshot
                .move_history
                .iter()
                .map(MoveRecordDto::from)
                .collect(),
            game_status: snapshot.game_status.into(),
            engine_status: (&snapshot.engine_status).into(),
            engine_limit: snapshot.engine_limit.into(),
        }
    }
}

/// Serialize a complete authoritative snapshot for the browser.
pub fn snapshot_to_json(snapshot: &GameSnapshot) -> String {
    // Serialization of the borrowing DTO cannot fail: every field is a plain scalar, string, or
    // sequence, none of which serde_json rejects.
    serde_json::to_string(&SnapshotDto::from(snapshot))
        .expect("wire snapshot is always serializable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::init::init_globals;
    use core::position::Position;
    use serde_json::Value;
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
            engine_limit: SearchLimit::Time(Duration::from_millis(1_500)),
        }
    }

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).expect("wire output is valid JSON")
    }

    #[test]
    fn serializes_a_snapshot_the_parser_accepts() {
        let mut snapshot = start_snapshot();
        snapshot.in_check = true;
        let json = snapshot_to_json(&snapshot);
        let value = parse(&json);
        assert_eq!(value.get("revision").unwrap().as_u64(), Some(3));
        assert_eq!(value.get("humanSide").unwrap().as_str(), Some("white"));
        assert_eq!(value.get("sideToMove").unwrap().as_str(), Some("white"));
        assert_eq!(value.get("inCheck"), Some(&Value::Bool(true)));
        assert_eq!(value.get("lastMove"), Some(&Value::Null));
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
        let moves = value.get("legalMoves").unwrap().as_array().unwrap();
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].as_str(), Some("e2e4"));
    }

    #[test]
    fn serializes_terminal_and_draw_statuses() {
        let mut snapshot = start_snapshot();
        snapshot.game_status = GameStatus::Checkmate {
            winner: Player::BLACK,
        };
        let value = parse(&snapshot_to_json(&snapshot));
        let status = value.get("gameStatus").unwrap();
        assert_eq!(status.get("kind").unwrap().as_str(), Some("checkmate"));
        assert_eq!(status.get("winner").unwrap().as_str(), Some("black"));

        for (reason, expected) in [
            (DrawReason::Stalemate, "stalemate"),
            (DrawReason::ThreefoldRepetition, "threefold_repetition"),
            (DrawReason::FiftyMoveRule, "fifty_move_rule"),
        ] {
            snapshot.game_status = GameStatus::Draw(reason);
            let value = parse(&snapshot_to_json(&snapshot));
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
            principal_variation_san: vec!["e4".to_owned()],
        };

        let value = parse(&snapshot_to_json(&snapshot));
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
        assert_eq!(score.get("centipawns").unwrap().as_i64(), Some(-42));
        let pv = progress
            .get("principalVariation")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(pv[0].as_str(), Some("e2e4"));
        let san = engine
            .get("principalVariationSan")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(san[0].as_str(), Some("e4"));
    }

    #[test]
    fn serializes_each_engine_limit_with_its_unit() {
        let mut snapshot = start_snapshot();

        snapshot.engine_limit = SearchLimit::Time(Duration::from_millis(2_500));
        let value = parse(&snapshot_to_json(&snapshot));
        let limit = value.get("engineLimit").unwrap();
        assert_eq!(limit.get("kind").unwrap().as_str(), Some("time"));
        assert_eq!(limit.get("milliseconds").unwrap().as_u64(), Some(2_500));

        snapshot.engine_limit = SearchLimit::Depth(6);
        let value = parse(&snapshot_to_json(&snapshot));
        let limit = value.get("engineLimit").unwrap();
        assert_eq!(limit.get("kind").unwrap().as_str(), Some("depth"));
        assert_eq!(limit.get("plies").unwrap().as_u64(), Some(6));

        // Not selectable from the browser, but the CLI default could name it and the snapshot
        // must stay parseable whatever the controller holds.
        snapshot.engine_limit = SearchLimit::Infinite;
        let value = parse(&snapshot_to_json(&snapshot));
        assert_eq!(
            value
                .get("engineLimit")
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str(),
            Some("infinite")
        );
    }

    #[test]
    fn accepts_engine_limits_inside_their_bounds_and_rejects_the_rest() {
        assert_eq!(
            parse_engine_limit("time", 1_000),
            Ok(SearchLimit::Time(Duration::from_millis(1_000)))
        );
        assert_eq!(
            parse_engine_limit("time", MIN_ENGINE_TIME_MS),
            Ok(SearchLimit::Time(Duration::from_millis(MIN_ENGINE_TIME_MS)))
        );
        assert_eq!(
            parse_engine_limit("time", MAX_ENGINE_TIME_MS),
            Ok(SearchLimit::Time(Duration::from_millis(MAX_ENGINE_TIME_MS)))
        );
        assert_eq!(parse_engine_limit("depth", 1), Ok(SearchLimit::Depth(1)));
        assert_eq!(
            parse_engine_limit("depth", MAX_ENGINE_DEPTH),
            Ok(SearchLimit::Depth(MAX_ENGINE_DEPTH as u8))
        );

        for (kind, value) in [
            ("time", MIN_ENGINE_TIME_MS - 1),
            ("time", MAX_ENGINE_TIME_MS + 1),
            ("time", 0),
            ("depth", 0),
            ("depth", MAX_ENGINE_DEPTH + 1),
            // An unbounded search would never produce a reply and would lock the board forever.
            ("infinite", 0),
            ("", 1),
            ("Time", 1_000),
        ] {
            assert_eq!(
                parse_engine_limit(kind, value),
                Err("invalid_engine_limit"),
                "{kind} {value} should be rejected"
            );
        }
    }

    #[test]
    fn reports_thinking_without_progress_as_null() {
        let mut snapshot = start_snapshot();
        snapshot.engine_status = EngineStatus::Thinking {
            search_id: 1,
            position_revision: 0,
            progress: None,
            principal_variation_san: Vec::new(),
        };
        let value = parse(&snapshot_to_json(&snapshot));
        assert_eq!(
            value.get("engineStatus").unwrap().get("progress"),
            Some(&Value::Null)
        );
        assert_eq!(
            value
                .get("engineStatus")
                .unwrap()
                .get("principalVariationSan"),
            Some(&Value::Array(Vec::new()))
        );
    }

    #[test]
    fn encodes_mate_scores_in_moves_matching_uci() {
        for moves in [1_i8, 3, 5] {
            let value = serde_json::to_value(ScoreDto::from(Score::mate(2 * moves - 1))).unwrap();
            assert_eq!(value.get("kind").unwrap().as_str(), Some("mate"));
            assert_eq!(
                value.get("moves").unwrap().as_i64(),
                Some(i64::from(moves)),
                "positive mate in {moves}"
            );

            let value = serde_json::to_value(ScoreDto::from(Score::mate(-2 * moves))).unwrap();
            assert_eq!(value.get("kind").unwrap().as_str(), Some("mate"));
            assert_eq!(
                value.get("moves").unwrap().as_i64(),
                Some(i64::from(-moves)),
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

    /// The infinities satisfy `is_mate` while sitting outside the mate band, so an is-mate-first
    /// ordering would run `INF_P` through the positive-mate conversion and render an infinite
    /// advantage as being mated. `Score`'s `Display` takes the infinities first; so does the DTO.
    #[test]
    fn infinite_scores_are_tagged_rather_than_converted_as_mates() {
        assert_eq!(
            serde_json::to_string(&ScoreDto::from(Score::INF_P)).unwrap(),
            r#"{"kind":"inf"}"#
        );
        assert_eq!(
            serde_json::to_string(&ScoreDto::from(Score::INF_N)).unwrap(),
            r#"{"kind":"-inf"}"#
        );

        // The guard is exact, so the mate band it sits above is untouched — the deepest mate this
        // engine represents still converts. `encodes_mate_scores_in_moves_matching_uci` covers the
        // band itself; this only shows the new branch does not swallow part of it.
        assert_eq!(
            serde_json::to_string(&ScoreDto::from(Score::mate(-98))).unwrap(),
            r#"{"kind":"mate","moves":-49}"#
        );
    }

    /// The browser reads the snapshot as a fixed byte layout: field order and value formatting are
    /// part of the contract, not merely the set of keys present. This pins the exact bytes serde
    /// emits for a representative snapshot, so a field reorder or a formatting change cannot pass
    /// silently past the structural assertions above.
    #[test]
    fn snapshot_serializes_to_the_exact_wire_bytes() {
        init_globals();
        let mut position = Position::start_pos();
        let mov = position.make_uci_move("e2e4").unwrap();
        let mut snapshot = start_snapshot();
        snapshot.last_move = Some(MoveRecord {
            uci: "e2e4".to_owned(),
            san: "e4".to_owned(),
        });
        snapshot.move_history = vec![MoveRecord {
            uci: "e2e4".to_owned(),
            san: "e4".to_owned(),
        }];
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
            principal_variation_san: vec!["e4".to_owned()],
        };

        let expected = concat!(
            r#"{"revision":3,"humanSide":"white","#,
            r#""fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1","#,
            r#""sideToMove":"white","inCheck":false,"#,
            r#""legalMoves":["e2e4","d2d4"],"#,
            r#""lastMove":{"uci":"e2e4","san":"e4"},"#,
            r#""moveHistory":[{"uci":"e2e4","san":"e4"}],"#,
            r#""gameStatus":{"kind":"ongoing"},"#,
            r#""engineStatus":{"kind":"thinking","searchId":7,"positionRevision":3,"#,
            r#""progress":{"depth":5,"score":{"kind":"cp","centipawns":-42},"#,
            r#""elapsedMs":250,"nodes":9000,"nps":36000,"hashfull":11,"#,
            r#""principalVariation":["e2e4"]},"principalVariationSan":["e4"]},"#,
            r#""engineLimit":{"kind":"time","milliseconds":1500}}"#,
        );
        assert_eq!(snapshot_to_json(&snapshot), expected);
    }
}
