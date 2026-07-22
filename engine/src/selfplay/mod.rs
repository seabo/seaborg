//! Self-play data generation: the engine plays against itself at a fixed, low
//! node budget per move, across many games at once, and records for each
//! searched position the search score and the eventual game outcome.
//!
//! This module owns the game loop, the win/draw/loss adjudication, and the
//! parallel orchestration across worker threads. It deliberately does *not* own
//! the on-disk sample encoding, position filtering, or opening diversification:
//! those are a separate concern so the label definitions here — a side-to-move
//! search score plus a win/draw/loss outcome — can be packed and filtered
//! without disturbing the game loop. The starting position is a parameter of
//! [`play_game`], so a reproducible node-budget search played from one fixed
//! start would reproduce a single game; [`run`] instead draws a diversified
//! start per game from [`openings`], and the loop itself needs no knowledge of
//! how that start was chosen.
//!
//! The concerns the game loop deliberately excludes live in sibling modules:
//! [`format`] is the compact on-disk encoding of a labelled position,
//! [`filter`] decides which positions are worth keeping, and [`openings`]
//! supplies the varied starting positions this loop plays out.

pub mod filter;
pub mod format;
pub mod openings;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chess::mono_traits::{All, Legal};
use chess::mov::Move;
use chess::movelist::BasicMoveList;
use chess::position::{PieceType, Player, Position};

use crate::nnue::Network;
use crate::score::Score;
use crate::search::{SearchEngine, SearchLimit};

/// A game result seen from a single position's side to move, matching the
/// reinforcement label `r`: a win scores 1, a draw 0.5, a loss 0.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Wdl {
    Win,
    Draw,
    Loss,
}

impl Wdl {
    /// The numeric label used as the game-outcome term of the training target.
    pub fn as_f32(self) -> f32 {
        match self {
            Wdl::Win => 1.0,
            Wdl::Draw => 0.5,
            Wdl::Loss => 0.0,
        }
    }
}

/// One retained self-play position with its two training labels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sample {
    /// The position the search scored. Its own side to move is the perspective
    /// every label below is measured from.
    pub position: Position,
    /// The engine's search score for `position`, from the side-to-move
    /// perspective. A centipawn score and a mate score are both preserved as
    /// they came from the search; mapping a mate onto the centipawn label is
    /// left to the sample encoder, which owns that band decision.
    pub score: Score,
    /// The eventual game outcome, from `position`'s side to move.
    pub outcome: Wdl,
    /// The move the search chose here, or `None` when the search returned no
    /// move (a terminal-adjacent position under a tiny budget). This is not a
    /// training label; it is retained so a filter can drop tactically unsettled
    /// positions — the ones whose best move is a capture — before the encoder
    /// discards it. The on-disk sample format does not store it.
    pub best_move: Option<Move>,
}

/// The result of a completed self-play game.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GameResult {
    /// The named player won.
    Win(Player),
    /// The game was drawn.
    Draw,
}

/// Why a self-play game ended.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Termination {
    Checkmate,
    Stalemate,
    ThreefoldRepetition,
    FiftyMoveRule,
    InsufficientMaterial,
    /// One side's evaluation stayed decisively winning for long enough that the
    /// game was called without playing the win out.
    Resignation,
    /// The evaluation stayed near zero for long enough to call a draw.
    DrawAdjudication,
    /// The safety cap on game length was reached; scored as a draw.
    MaxPlies,
}

/// One completed self-play game: its retained positions with labels, its result,
/// and how it ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameRecord {
    pub samples: Vec<Sample>,
    pub result: GameResult,
    pub termination: Termination,
}

/// Rules for ending a game early from the engine's own evaluation, so a decided
/// or dead-drawn game is not played all the way to the fifty-move rule.
#[derive(Copy, Clone, Debug)]
pub struct Adjudication {
    /// A side leading by at least this many centipawns (in its own favour) for
    /// [`resign_plies`](Self::resign_plies) consecutive plies wins by
    /// resignation. Mate scores exceed any centipawn margin, so a found mate
    /// adjudicates here rather than being played out.
    pub resign_score_cp: i32,
    /// Consecutive plies the resign margin must hold before the game is called.
    pub resign_plies: u32,
    /// An evaluation within this many centipawns of zero for
    /// [`draw_plies`](Self::draw_plies) consecutive plies draws the game.
    pub draw_score_cp: i32,
    /// Consecutive plies the draw margin must hold before the game is called.
    pub draw_plies: u32,
    /// Draw adjudication is suppressed until this ply, so quiet opening
    /// positions cannot end a game before it has developed.
    pub draw_min_ply: usize,
}

impl Default for Adjudication {
    fn default() -> Self {
        Self {
            resign_score_cp: 1_000,
            resign_plies: 4,
            draw_score_cp: 8,
            draw_plies: 8,
            draw_min_ply: 40,
        }
    }
}

/// Configuration for a self-play data-generation run.
#[derive(Clone, Debug)]
pub struct SelfPlayConfig {
    /// Node budget searched for every move. A node budget (rather than time or
    /// depth) makes the generated labels reproducible across machines and builds.
    pub node_budget: u64,
    /// Number of concurrent worker threads, each running one single-threaded
    /// search at a time.
    pub workers: usize,
    /// Total number of games to play.
    pub games: usize,
    /// Transposition-table size, in megabytes, for each worker's engine.
    pub hash_size_mb: usize,
    /// Hard cap on game length in plies, so a game can never fail to terminate.
    /// A game reaching the cap is scored as a draw.
    pub max_plies: usize,
    /// Early-termination thresholds.
    pub adjudication: Adjudication,
    /// How each game's starting position is diversified away from the initial
    /// position, so the generated games do not all repeat one opening.
    pub opening: openings::OpeningConfig,
    /// The network the self-play searches evaluate with, or `None` for the
    /// hand-crafted evaluation.
    ///
    /// `None` is the reinforcement loop's generation-0 bootstrap: the first
    /// self-play data is labelled by the engine playing with only its
    /// hand-crafted evaluation. Each later generation sets the previous
    /// generation's promoted network here, so the games that produce a
    /// generation's labels are played by the engine using the prior network —
    /// the self-play purity boundary the design contract requires. Shared behind
    /// an [`Arc`] so every worker's engine references the one loaded copy.
    pub network: Option<Arc<Network>>,
}

impl Default for SelfPlayConfig {
    fn default() -> Self {
        Self {
            node_budget: 5_000,
            workers: 1,
            games: 1,
            hash_size_mb: 16,
            max_plies: 800,
            adjudication: Adjudication::default(),
            opening: openings::OpeningConfig::default(),
            network: None,
        }
    }
}

/// Aggregate throughput of a completed run, so training-cost estimates can be
/// checked against measured reality.
#[derive(Copy, Clone, Debug)]
pub struct ThroughputReport {
    pub games: usize,
    pub positions: usize,
    pub elapsed: Duration,
    pub positions_per_second: f64,
}

/// Classify a position's natural ending, recognised from the board alone before
/// any search. Returns `None` for a position where the game continues.
fn terminal_status(position: &Position) -> Option<(GameResult, Termination)> {
    let moves = position.generate::<BasicMoveList, All, Legal>();
    if moves.is_empty() {
        // No legal move: checkmate if the side to move is in check, else a
        // stalemate draw. Mate takes priority over every drawing rule below.
        return Some(if position.in_check() {
            (GameResult::Win(!position.turn()), Termination::Checkmate)
        } else {
            (GameResult::Draw, Termination::Stalemate)
        });
    }
    if position.in_threefold() {
        return Some((GameResult::Draw, Termination::ThreefoldRepetition));
    }
    if position.fifty_move_rule_reached() {
        return Some((GameResult::Draw, Termination::FiftyMoveRule));
    }
    if is_insufficient_material(position) {
        return Some((GameResult::Draw, Termination::InsufficientMaterial));
    }
    None
}

/// The strictly uncontroversial dead positions: no pawns, rooks, or queens on
/// the board and at most one minor piece in total, so no legal sequence can
/// deliver mate (king versus king, king versus lone knight, king versus lone
/// bishop). Harder theoretical draws such as opposite-coloured lone bishops are
/// deliberately not encoded; the fifty-move rule ends those instead.
fn is_insufficient_material(position: &Position) -> bool {
    for player in [Player::WHITE, Player::BLACK] {
        for pt in [PieceType::Pawn, PieceType::Rook, PieceType::Queen] {
            if position.piece_bb(player, pt).popcnt() != 0 {
                return false;
            }
        }
    }
    let minors: u32 = [Player::WHITE, Player::BLACK]
        .into_iter()
        .flat_map(|player| [PieceType::Knight, PieceType::Bishop].map(move |pt| (player, pt)))
        .map(|(player, pt)| position.piece_bb(player, pt).popcnt())
        .sum();
    minors <= 1
}

/// Tracks how long the evaluation has stayed decisive or near zero, in
/// White's-eye centipawns, to apply the resign and draw adjudication rules.
struct Adjudicator {
    rules: Adjudication,
    /// Signed run length of consecutive plies one side has met the resign
    /// margin: positive counts White-winning plies, negative Black-winning,
    /// reset to zero on any ply that meets neither.
    resign_run: i32,
    /// Consecutive plies the evaluation has sat inside the draw margin.
    draw_run: u32,
}

impl Adjudicator {
    fn new(rules: Adjudication) -> Self {
        Self {
            rules,
            resign_run: 0,
            draw_run: 0,
        }
    }

    /// Feed the search score for the position at `ply` (0-based), expressed in
    /// White's-eye centipawns, and return an adjudicated ending if a rule fires.
    fn observe(&mut self, ply: usize, white_cp: i32) -> Option<(GameResult, Termination)> {
        if white_cp >= self.rules.resign_score_cp {
            self.resign_run = if self.resign_run > 0 {
                self.resign_run + 1
            } else {
                1
            };
        } else if white_cp <= -self.rules.resign_score_cp {
            self.resign_run = if self.resign_run < 0 {
                self.resign_run - 1
            } else {
                -1
            };
        } else {
            self.resign_run = 0;
        }
        if self.resign_run.unsigned_abs() >= self.rules.resign_plies {
            let winner = if self.resign_run > 0 {
                Player::WHITE
            } else {
                Player::BLACK
            };
            return Some((GameResult::Win(winner), Termination::Resignation));
        }

        if ply >= self.rules.draw_min_ply && white_cp.abs() <= self.rules.draw_score_cp {
            self.draw_run += 1;
        } else {
            self.draw_run = 0;
        }
        if self.draw_run >= self.rules.draw_plies {
            return Some((GameResult::Draw, Termination::DrawAdjudication));
        }

        None
    }
}

/// The win/draw/loss label a finished game hands to a position whose side to
/// move is `side`.
fn outcome_for(result: GameResult, side: Player) -> Wdl {
    match result {
        GameResult::Draw => Wdl::Draw,
        GameResult::Win(winner) => {
            if winner == side {
                Wdl::Win
            } else {
                Wdl::Loss
            }
        }
    }
}

/// Play one self-play game from `start` and return its record.
///
/// `engine` supplies the transposition table reused across this game's moves;
/// the caller is responsible for clearing it between games. Every move is
/// searched to `config.node_budget` nodes, so the game is reproducible on a
/// given build.
pub fn play_game(engine: &SearchEngine, start: Position, config: &SelfPlayConfig) -> GameRecord {
    let mut position = start;
    // Positions searched, with their side-to-move scores and chosen moves; the
    // outcome label is filled in once the game result is known.
    let mut scored: Vec<(Position, Score, Option<Move>)> = Vec::new();
    let mut adjudicator = Adjudicator::new(config.adjudication);

    let (result, termination) = loop {
        if let Some(ending) = terminal_status(&position) {
            break ending;
        }
        let ply = scored.len();
        if ply >= config.max_plies {
            break (GameResult::Draw, Termination::MaxPlies);
        }

        let outcome = engine
            .start(position.clone(), SearchLimit::Nodes(config.node_budget))
            .wait();
        let (score, best_move) = match outcome.result() {
            Some(result) => (result.score, result.best_move),
            None => (Score::zero(), None),
        };

        // Record the scored position before playing on: the label belongs to
        // this position, with its own side to move.
        scored.push((position.clone(), score, best_move));

        let white_cp = {
            let stm_cp = i32::from(score.to_i16());
            if position.turn() == Player::WHITE {
                stm_cp
            } else {
                -stm_cp
            }
        };
        if let Some(ending) = adjudicator.observe(ply, white_cp) {
            break ending;
        }

        // A non-terminal position always has a legal move, and the search
        // guarantees one even under a tiny budget; fall back to the first legal
        // move only so a game can never stall on an absent result.
        let mov = best_move.unwrap_or_else(|| first_legal_move(&position));
        position.make_move(&mov);
    };

    let samples = scored
        .into_iter()
        .map(|(position, score, best_move)| {
            let outcome = outcome_for(result, position.turn());
            Sample {
                position,
                score,
                outcome,
                best_move,
            }
        })
        .collect();

    GameRecord {
        samples,
        result,
        termination,
    }
}

/// The first legal move in a position known to have one.
fn first_legal_move(position: &Position) -> Move {
    let moves = position.generate::<BasicMoveList, All, Legal>();
    (&moves)
        .into_iter()
        .next()
        .copied()
        .expect("a non-terminal position always has a legal move")
}

/// Play `config.games` self-play games across `config.workers` threads, handing
/// each completed game to `sink`, and return aggregate throughput.
///
/// Each worker owns a private [`SearchEngine`], and therefore a private
/// transposition table, so the single-threaded searches in different workers
/// never contend. Games are pulled from a shared counter, so uneven game lengths
/// still balance across workers. `sink` is invoked on the calling thread as
/// records arrive, which keeps it free of any `Send` requirement and lets a
/// consumer stream the records without its own synchronisation.
pub fn run<F>(config: &SelfPlayConfig, mut sink: F) -> ThroughputReport
where
    F: FnMut(GameRecord),
{
    let workers = config.workers.max(1);
    let next_game = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel::<GameRecord>();

    let start = Instant::now();

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let next_game = Arc::clone(&next_game);
        let tx = tx.clone();
        let config = config.clone();
        handles.push(std::thread::spawn(move || {
            let mut engine = SearchEngine::new(config.hash_size_mb);
            // Every game this worker plays evaluates with the configured network (the previous
            // generation's, or the hand-crafted evaluation at generation 0). Set once here rather
            // than per game: `new_game` only clears the shared table, not the evaluator.
            engine.set_network(config.network.clone());
            loop {
                let index = next_game.fetch_add(1, Ordering::Relaxed);
                if index >= config.games {
                    break;
                }
                // A fresh table per game keeps the games independent of one
                // another and reproducible in isolation.
                engine.new_game();
                // The start is chosen from the game index alone, so which games
                // a worker happens to pull never changes their openings.
                let start = config.opening.start_for(index);
                let record = play_game(&engine, start, &config);
                if tx.send(record).is_err() {
                    break;
                }
            }
        }));
    }
    // Drop the extra sender so the receiver loop below ends once, and only once,
    // every worker's sender has been dropped.
    drop(tx);

    let mut positions = 0usize;
    let mut games = 0usize;
    for record in rx {
        positions += record.samples.len();
        games += 1;
        sink(record);
    }

    for handle in handles {
        // A worker only searches and sends, so a panic there is a bug worth
        // surfacing rather than swallowing.
        handle.join().expect("self-play worker panicked");
    }

    let elapsed = start.elapsed();
    let seconds = elapsed.as_secs_f64();
    let positions_per_second = if seconds > 0.0 {
        positions as f64 / seconds
    } else {
        0.0
    };

    ThroughputReport {
        games,
        positions,
        elapsed,
        positions_per_second,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(fen: &str) -> Position {
        Position::from_fen(fen).expect("valid FEN")
    }

    #[test]
    fn wdl_labels_match_training_convention() {
        assert_eq!(Wdl::Win.as_f32(), 1.0);
        assert_eq!(Wdl::Draw.as_f32(), 0.5);
        assert_eq!(Wdl::Loss.as_f32(), 0.0);
    }

    #[test]
    fn checkmate_is_a_win_for_the_mating_side() {
        // Fool's mate: White to move is checkmated, so Black has won.
        let mated = position("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(
            terminal_status(&mated),
            Some((GameResult::Win(Player::BLACK), Termination::Checkmate))
        );
    }

    #[test]
    fn stalemate_is_a_draw() {
        // Black to move, not in check, with no legal move.
        let stalemate = position("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        assert_eq!(
            terminal_status(&stalemate),
            Some((GameResult::Draw, Termination::Stalemate))
        );
    }

    #[test]
    fn fifty_move_rule_is_a_draw() {
        let fifty = position("7k/8/8/8/8/8/8/K7 w - - 100 51");
        assert_eq!(
            terminal_status(&fifty),
            Some((GameResult::Draw, Termination::FiftyMoveRule))
        );
    }

    #[test]
    fn threefold_repetition_is_a_draw() {
        // Shuffle both knights out and back twice, returning the start position
        // to the board for the third time.
        let mut pos = Position::start_pos();
        for uci in [
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ] {
            assert!(
                pos.make_uci_move(uci).is_some(),
                "move {uci} should be legal"
            );
        }
        assert_eq!(
            terminal_status(&pos),
            Some((GameResult::Draw, Termination::ThreefoldRepetition))
        );
    }

    #[test]
    fn bare_kings_are_insufficient_material() {
        assert!(is_insufficient_material(&position(
            "8/8/4k3/8/8/4K3/8/8 w - - 0 1"
        )));
    }

    #[test]
    fn lone_minor_is_insufficient_material() {
        assert!(is_insufficient_material(&position(
            "8/8/4k3/8/8/4K3/5N2/8 w - - 0 1"
        )));
        assert!(is_insufficient_material(&position(
            "8/8/4k3/8/8/4K3/5B2/8 w - - 0 1"
        )));
    }

    #[test]
    fn queen_or_two_minors_are_sufficient_material() {
        assert!(!is_insufficient_material(&position(
            "8/8/4k3/8/8/4K3/5Q2/8 w - - 0 1"
        )));
        // Two minor pieces can force mate, so they are not a dead position.
        assert!(!is_insufficient_material(&position(
            "8/8/4k3/8/8/4K3/4BN2/8 w - - 0 1"
        )));
    }

    #[test]
    fn resign_adjudicates_after_the_margin_holds() {
        let mut adj = Adjudicator::new(Adjudication::default());
        // Three plies at the margin are not enough with the default four.
        assert_eq!(adj.observe(0, 2_000), None);
        assert_eq!(adj.observe(1, 2_000), None);
        assert_eq!(adj.observe(2, 2_000), None);
        assert_eq!(
            adj.observe(3, 2_000),
            Some((GameResult::Win(Player::WHITE), Termination::Resignation))
        );
    }

    #[test]
    fn resign_run_resets_when_the_margin_breaks() {
        let mut adj = Adjudicator::new(Adjudication::default());
        assert_eq!(adj.observe(0, 2_000), None);
        assert_eq!(adj.observe(1, 2_000), None);
        // The advantage evaporates, so the run restarts from scratch.
        assert_eq!(adj.observe(2, 0), None);
        assert_eq!(adj.observe(3, 2_000), None);
        assert_eq!(adj.observe(4, 2_000), None);
        assert_eq!(adj.observe(5, 2_000), None);
        assert_eq!(
            adj.observe(6, 2_000),
            Some((GameResult::Win(Player::WHITE), Termination::Resignation))
        );
    }

    #[test]
    fn a_losing_side_resigns_to_its_opponent() {
        let mut adj = Adjudicator::new(Adjudication::default());
        for ply in 0..3 {
            assert_eq!(adj.observe(ply, -2_000), None);
        }
        assert_eq!(
            adj.observe(3, -2_000),
            Some((GameResult::Win(Player::BLACK), Termination::Resignation))
        );
    }

    #[test]
    fn draw_adjudication_waits_for_the_minimum_ply() {
        let mut adj = Adjudicator::new(Adjudication::default());
        // Near-zero scores before the minimum ply must not accumulate.
        for ply in 0..30 {
            assert_eq!(adj.observe(ply, 0), None);
        }
        // Once past the minimum, eight consecutive quiet plies draw.
        for ply in 40..47 {
            assert_eq!(adj.observe(ply, 0), None);
        }
        assert_eq!(
            adj.observe(47, 0),
            Some((GameResult::Draw, Termination::DrawAdjudication))
        );
    }

    #[test]
    fn play_game_reaches_checkmate_with_the_right_winner() {
        // Ra8 is mate in one: the black king is boxed in by its own pawns. Give
        // the search a generous budget so it finds the mate, and disable
        // adjudication so the checkmate itself ends the game.
        let config = SelfPlayConfig {
            node_budget: 200_000,
            max_plies: 8,
            adjudication: Adjudication {
                resign_plies: u32::MAX,
                draw_plies: u32::MAX,
                draw_min_ply: usize::MAX,
                ..Adjudication::default()
            },
            ..SelfPlayConfig::default()
        };
        let engine = SearchEngine::new(config.hash_size_mb);
        let start = position("6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1");
        let record = play_game(&engine, start, &config);

        assert_eq!(record.termination, Termination::Checkmate);
        assert_eq!(record.result, GameResult::Win(Player::WHITE));
        // The single scored position had White to move and White won it.
        assert_eq!(record.samples.len(), 1);
        assert_eq!(record.samples[0].position.turn(), Player::WHITE);
        assert_eq!(record.samples[0].outcome, Wdl::Win);
    }

    #[test]
    fn play_game_is_reproducible_on_the_same_build() {
        let config = SelfPlayConfig {
            node_budget: 2_000,
            max_plies: 24,
            ..SelfPlayConfig::default()
        };
        let first = {
            let engine = SearchEngine::new(config.hash_size_mb);
            play_game(&engine, Position::start_pos(), &config)
        };
        let second = {
            let engine = SearchEngine::new(config.hash_size_mb);
            play_game(&engine, Position::start_pos(), &config)
        };
        assert_eq!(first, second);
    }

    #[test]
    fn run_threads_the_configured_network_into_self_play() {
        // The committed golden network is a valid SBNN file. Path relative to the package
        // directory, the working directory of a cargo test binary.
        let bytes = std::fs::read("tests/fixtures/golden_v1.sbnn")
            .expect("committed golden network fixture is readable");
        let network =
            Arc::new(Network::read(&mut &bytes[..]).expect("golden fixture is a valid network"));

        // One game, one worker, a fixed opening and node budget, so the only thing that can make the
        // two runs differ is the evaluator each played with.
        let base = SelfPlayConfig {
            node_budget: 2_000,
            workers: 1,
            games: 1,
            max_plies: 24,
            ..SelfPlayConfig::default()
        };
        let play = |config: &SelfPlayConfig| {
            let mut collected = Vec::new();
            run(config, |record| collected.push(record));
            collected
        };

        let handcrafted = play(&base);
        let networked = play(&SelfPlayConfig {
            network: Some(network),
            ..base.clone()
        });

        assert_eq!(handcrafted.len(), 1);
        assert_eq!(networked.len(), 1);
        // An identical game would mean the configured network never reached the workers; the games
        // differing (in moves or in the recorded search scores) is the evaluator taking effect.
        assert_ne!(handcrafted[0], networked[0]);
    }

    /// Self-play with no configured network plays the hand-crafted evaluation even in a build that
    /// carries one.
    ///
    /// A fresh `SearchEngine` evaluates with the built-in network, which is right for playing but
    /// wrong for generating training data: the bootstrap generation's labels must come from the
    /// hand-crafted evaluation, and a network leaking in through the constructor would corrupt them
    /// invisibly — the data would still look well-formed. Self-play therefore sets the evaluator
    /// from its own config unconditionally, and this pins that.
    #[test]
    fn a_config_without_a_network_plays_the_hand_crafted_evaluation() {
        let Some(built_in) = crate::nnue::built_in_network() else {
            // Nothing can leak in a build with no built-in network; the risk this test guards
            // against does not exist there.
            return;
        };

        let base = SelfPlayConfig {
            node_budget: 2_000,
            workers: 1,
            games: 1,
            max_plies: 24,
            ..SelfPlayConfig::default()
        };
        let play = |config: &SelfPlayConfig| {
            let mut collected = Vec::new();
            run(config, |record| collected.push(record));
            collected
        };

        let unconfigured = play(&base);
        let with_built_in = play(&SelfPlayConfig {
            network: Some(built_in),
            ..base.clone()
        });

        // Identical games would mean the unconfigured run had in fact used the built-in network.
        assert_ne!(unconfigured[0], with_built_in[0]);
    }

    #[test]
    fn run_plays_every_game_and_measures_throughput() {
        let config = SelfPlayConfig {
            node_budget: 1_000,
            workers: 2,
            games: 4,
            max_plies: 16,
            ..SelfPlayConfig::default()
        };
        let mut collected = Vec::new();
        let report = run(&config, |record| collected.push(record));

        assert_eq!(report.games, 4);
        assert_eq!(collected.len(), 4);
        let positions: usize = collected.iter().map(|record| record.samples.len()).sum();
        assert_eq!(report.positions, positions);
        assert!(report.positions > 0);
        assert!(report.positions_per_second > 0.0);
        // Every retained sample carries both labels; the outcome is consistent
        // with the game result and the position's side to move.
        for record in &collected {
            for sample in &record.samples {
                assert_eq!(
                    sample.outcome,
                    outcome_for(record.result, sample.position.turn())
                );
            }
        }
    }
}
