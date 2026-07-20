//! The `datagen` subcommand: generate self-play training data at a fixed node
//! budget per move and report throughput.
//!
//! The engine owns the game loop, adjudication, and parallel orchestration
//! (`engine::selfplay`); this module only parses configuration and prints a
//! summary. The generated samples are dropped rather than written: the packed
//! on-disk format that persists them is a separate concern, so today's binary
//! measures throughput and adjudication behaviour, which is what validates the
//! training-cost estimates.

use engine::selfplay::{self, Adjudication, GameResult, SelfPlayConfig, Termination};

/// Arguments for `seaborg datagen`.
#[derive(Debug, clap::Args)]
pub struct DatagenArgs {
    /// Node budget searched for every move
    #[clap(long, default_value_t = 5_000)]
    nodes: u64,

    /// Total number of games to play
    #[clap(long, default_value_t = 100)]
    games: usize,

    /// Number of concurrent worker threads (defaults to available parallelism)
    #[clap(long)]
    workers: Option<usize>,

    /// Transposition-table size per worker, in megabytes
    #[clap(long, default_value_t = 16)]
    hash: usize,

    /// Hard cap on game length in plies, scored as a draw when reached
    #[clap(long, default_value_t = 800)]
    max_plies: usize,

    /// Centipawn margin a side must hold to win by resignation
    #[clap(long, default_value_t = 1_000)]
    resign_score: i32,

    /// Consecutive plies the resign margin must hold
    #[clap(long, default_value_t = 4)]
    resign_plies: u32,

    /// Centipawn distance from zero that counts as drawish
    #[clap(long, default_value_t = 8)]
    draw_score: i32,

    /// Consecutive plies the draw margin must hold
    #[clap(long, default_value_t = 8)]
    draw_plies: u32,

    /// Earliest ply at which a draw may be adjudicated
    #[clap(long, default_value_t = 40)]
    draw_min_ply: usize,
}

/// Running counts over the games of a run, so the summary can show the result
/// and termination mix alongside the throughput.
#[derive(Default)]
struct Tally {
    white_wins: u64,
    black_wins: u64,
    draws: u64,
    checkmate: u64,
    stalemate: u64,
    threefold: u64,
    fifty_move: u64,
    insufficient: u64,
    resignation: u64,
    draw_adjudication: u64,
    max_plies: u64,
}

impl Tally {
    fn record(&mut self, result: GameResult, termination: Termination) {
        match result {
            GameResult::Win(player) if player.is_white() => self.white_wins += 1,
            GameResult::Win(_) => self.black_wins += 1,
            GameResult::Draw => self.draws += 1,
        }
        match termination {
            Termination::Checkmate => self.checkmate += 1,
            Termination::Stalemate => self.stalemate += 1,
            Termination::ThreefoldRepetition => self.threefold += 1,
            Termination::FiftyMoveRule => self.fifty_move += 1,
            Termination::InsufficientMaterial => self.insufficient += 1,
            Termination::Resignation => self.resignation += 1,
            Termination::DrawAdjudication => self.draw_adjudication += 1,
            Termination::MaxPlies => self.max_plies += 1,
        }
    }
}

pub fn datagen(args: &DatagenArgs) {
    let workers = args
        .workers
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));

    let config = SelfPlayConfig {
        node_budget: args.nodes,
        workers,
        games: args.games,
        hash_size_mb: args.hash,
        max_plies: args.max_plies,
        adjudication: Adjudication {
            resign_score_cp: args.resign_score,
            resign_plies: args.resign_plies,
            draw_score_cp: args.draw_score,
            draw_plies: args.draw_plies,
            draw_min_ply: args.draw_min_ply,
        },
    };

    println!(
        "Self-play: {} games, {} workers, {} nodes/move",
        config.games, config.workers, config.node_budget
    );

    let mut tally = Tally::default();
    let report = selfplay::run(&config, |record| {
        tally.record(record.result, record.termination);
    });

    let seconds = report.elapsed.as_secs_f64();
    println!(
        "Played {} games ({} positions) in {seconds:.1}s: {:.0} positions/s",
        report.games, report.positions, report.positions_per_second
    );
    println!(
        "Results: {} white wins, {} black wins, {} draws",
        tally.white_wins, tally.black_wins, tally.draws
    );
    println!(
        "Endings: {} checkmate, {} stalemate, {} threefold, {} fifty-move, \
         {} insufficient material, {} resignation, {} draw adjudication, {} max plies",
        tally.checkmate,
        tally.stalemate,
        tally.threefold,
        tally.fifty_move,
        tally.insufficient,
        tally.resignation,
        tally.draw_adjudication,
        tally.max_plies,
    );
}
