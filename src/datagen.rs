//! The `datagen` subcommand: generate self-play training data at a fixed node
//! budget per move and report throughput.
//!
//! The engine owns the game loop, adjudication, and parallel orchestration
//! (`engine::selfplay`); this module parses configuration, optionally writes the
//! packed on-disk samples, and prints a summary. With no `--out` path the
//! samples are dropped and the run just measures throughput and adjudication
//! behaviour, which is what validates the training-cost estimates.

use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::Arc;

use engine::nnue::Network;
use engine::selfplay::filter::PositionFilter;
use engine::selfplay::format::SampleWriter;
use engine::selfplay::openings::OpeningConfig;
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

    /// Write packed training samples to this file (samples are dropped if unset)
    #[clap(long)]
    out: Option<PathBuf>,

    /// Random legal plies played from the initial position to diversify each
    /// game's opening (defaults to the engine's built-in setting)
    #[clap(long)]
    opening_plies: Option<usize>,

    /// Seed for opening diversification (defaults to the engine's built-in seed)
    #[clap(long)]
    opening_seed: Option<u64>,

    /// Keep positions whose side to move is in check (dropped by default)
    #[clap(long)]
    keep_in_check: bool,

    /// Keep positions whose best move is a capture (dropped by default)
    #[clap(long)]
    keep_captures: bool,

    /// Drop the first this-many plies of each game as near-book
    #[clap(long, default_value_t = 0)]
    filter_opening_plies: usize,

    /// Evaluate self-play with this `SBNN` network instead of the hand-crafted
    /// evaluation. Unset is the reinforcement loop's generation-0 bootstrap, and
    /// each later generation passes the previous generation's promoted network so
    /// the games are labelled by the engine playing with it.
    #[clap(long)]
    network: Option<PathBuf>,
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

    let default_opening = OpeningConfig::default();
    let opening = OpeningConfig {
        plies: args.opening_plies.unwrap_or(default_opening.plies),
        seed: args.opening_seed.unwrap_or(default_opening.seed),
    };

    // Load the evaluator network before any games run so a bad path or malformed file fails the
    // whole run up front rather than after generating samples. Absent means generation-0 bootstrap:
    // the engine plays with its hand-crafted evaluation.
    let network = match args.network.as_ref() {
        Some(path) => match load_network(path) {
            Ok(network) => Some(Arc::new(network)),
            Err(e) => {
                eprintln!("Could not load network {}: {e}", path.display());
                return;
            }
        },
        None => None,
    };

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
        opening,
        network,
    };

    let filter = PositionFilter {
        skip_in_check: !args.keep_in_check,
        skip_best_move_capture: !args.keep_captures,
        skip_opening_plies: args.filter_opening_plies,
    };

    // Open the output stream up front so a bad path fails before any games run.
    let mut writer = match args.out.as_ref() {
        Some(path) => match open_writer(path) {
            Ok(writer) => Some(writer),
            Err(e) => {
                eprintln!("Could not open {}: {e}", path.display());
                return;
            }
        },
        None => None,
    };

    println!(
        "Self-play: {} games, {} workers, {} nodes/move, evaluator: {}",
        config.games,
        config.workers,
        config.node_budget,
        match args.network.as_ref() {
            Some(path) => path.display().to_string(),
            None => "hand-crafted".to_string(),
        }
    );

    let mut tally = Tally::default();
    // The sink runs on the calling thread, so writing needs no synchronisation.
    // A write failure is latched and stops further writes; the run still drains
    // so worker threads are always joined cleanly.
    let mut retained = 0u64;
    let mut write_err: Option<io::Error> = None;
    let report = selfplay::run(&config, |record| {
        tally.record(record.result, record.termination);
        for sample in filter.retained(&record) {
            retained += 1;
            if let Some(writer) = writer.as_mut() {
                if write_err.is_none() {
                    if let Err(e) = writer.write_sample(sample) {
                        write_err = Some(e);
                    }
                }
            }
        }
    });

    if let Some(writer) = writer {
        if write_err.is_none() {
            // `BufWriter::into_inner` flushes the buffer; surface a flush failure
            // rather than dropping buffered samples silently.
            if let Err(e) = writer.into_inner().into_inner() {
                write_err = Some(e.into_error());
            }
        }
    }

    let seconds = report.elapsed.as_secs_f64();
    println!(
        "Played {} games ({} positions) in {seconds:.1}s: {:.0} positions/s",
        report.games, report.positions, report.positions_per_second
    );
    if let Some(path) = args.out.as_ref() {
        match &write_err {
            None => println!(
                "Wrote {retained} filtered samples ({} dropped) to {}",
                report.positions as u64 - retained,
                path.display()
            ),
            Some(e) => eprintln!(
                "Write to {} failed after {retained} samples: {e}",
                path.display()
            ),
        }
    } else {
        println!(
            "Retained {retained} of {} positions after filtering (not written)",
            report.positions
        );
    }
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

/// Open `path` for writing and start a sample stream, buffering the file so each
/// small fixed-size record is not its own write syscall.
fn open_writer(path: &PathBuf) -> io::Result<SampleWriter<BufWriter<File>>> {
    let file = File::create(path)?;
    SampleWriter::new(BufWriter::new(file))
}

/// Load and validate an `SBNN` network file for the self-play evaluator.
///
/// Buffered because the loader reads the header and parameter blob in many small reads. A
/// filesystem failure and a rejected file are surfaced as one error so the caller can report either
/// cause identically.
fn load_network(path: &PathBuf) -> Result<Network, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(File::open(path)?);
    Ok(Network::read(&mut reader)?)
}
