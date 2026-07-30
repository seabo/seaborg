//! Generate the rules-verified forced-mate suite from Seaborg self-play.
//!
//! This builds the ground-truth set the mate-find-rate diagnostic
//! (`tools/diag/mate_find_rate.py`) measures against. Every position in the
//! output is a position the *rules* prove is a forced mate for the side to move,
//! sourced entirely from Seaborg's own play — no tablebase, opening book, or
//! other engine is consulted.
//!
//! # Pipeline
//!
//! 1. **Source.** Play self-play games with [`engine::selfplay`] (single worker,
//!    fixed opening seed, so a run is reproducible) and consider every position
//!    the games searched.
//! 2. **Triage.** The exact solver in step 3 is a complete minimax and is only
//!    cheap where a short mate actually exists; running it on every quiet
//!    position would be wasteful. So a position is only *attempted* when the
//!    self-play search already scored it as a decisive win for the side to move.
//!    This is a speed filter only: it decides which positions are worth proving,
//!    never whether a mate is real. The search is not trusted for correctness.
//! 3. **Prove.** [`engine::mate::MateSolver`] proves, from the rules alone,
//!    whether the side to move forces mate and in how many plies. A position is
//!    kept only on a positive proof within the ply and node bounds. Enumerating
//!    every defender reply *is* the brute-force forced-mate proof; the solver
//!    shares no code with the search under test.
//! 4. **Cross-check.** The proven optimal line is replayed with
//!    [`engine::mate::playout_is_mate`] — a structurally different, purely
//!    mechanical check that the line ends in checkmate — as independent
//!    confirmation before the position is admitted.
//! 5. **Balance and write.** Positions are capped per mate-distance bucket so no
//!    single distance dominates, then written as JSON.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p engine --example mate_suite_gen -- \
//!     --games 400 --nodes 40000 --max-mate-plies 7 --per-bucket 40 \
//!     --out suites/mate_suite.json
//! ```

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use engine::mate::{playout_is_mate, MateResult, MateSolver};
use engine::nnue::Network;
use engine::score::Score;
use engine::selfplay::openings::OpeningConfig;
use engine::selfplay::{self, Adjudication, SelfPlayConfig};
use serde::Serialize;

/// One rules-verified forced mate in the suite.
#[derive(Serialize)]
struct SuitePosition {
    /// The position, side to move being the mating side.
    fen: String,
    /// Proven minimal forced-mate distance in plies (half-moves); always odd.
    mate_plies: u32,
    /// The same distance expressed in moves (`(mate_plies + 1) / 2`), the more
    /// familiar "mate in N".
    mate_moves: u32,
    /// Every first move that keeps a forced mate within `mate_plies`. The
    /// diagnostic counts the engine as having solved the position when its chosen
    /// move is one of these — a rules-verified success criterion.
    winning_first_moves: Vec<String>,
    /// One optimal mating line (attacker's fastest mate against the defender's
    /// longest resistance), retained so the mate is independently replayable.
    line: Vec<String>,
}

/// The written suite: its provenance and generation parameters alongside the
/// positions, so the artefact records exactly how it was produced.
#[derive(Serialize)]
struct Suite {
    description: String,
    source: String,
    params: Params,
    counts_by_mate_moves: BTreeMap<u32, usize>,
    positions: Vec<SuitePosition>,
}

#[derive(Serialize)]
struct Params {
    games: usize,
    self_play_nodes: u64,
    max_mate_plies: u32,
    solver_node_cap: u64,
    per_bucket: usize,
    triage_cp: i16,
    opening_seed: u64,
    opening_plies: usize,
    hash_mb: usize,
    network: String,
}

struct Args {
    games: usize,
    nodes: u64,
    max_mate_plies: u32,
    node_cap: u64,
    per_bucket: usize,
    triage_cp: i16,
    seed: u64,
    opening_plies: Option<usize>,
    hash_mb: usize,
    network: Option<PathBuf>,
    out: PathBuf,
}

impl Args {
    fn parse() -> Args {
        let default_opening = OpeningConfig::default();
        let mut args = Args {
            games: 400,
            nodes: 40_000,
            max_mate_plies: 7,
            node_cap: 400_000,
            per_bucket: 40,
            triage_cp: 500,
            seed: default_opening.seed,
            opening_plies: None,
            hash_mb: 16,
            network: None,
            out: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../suites/mate_suite.json"
            )),
        };
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let mut value = || {
                it.next()
                    .unwrap_or_else(|| panic!("missing value for {flag}"))
            };
            match flag.as_str() {
                "--games" => args.games = value().parse().expect("--games"),
                "--nodes" => args.nodes = value().parse().expect("--nodes"),
                "--max-mate-plies" => {
                    args.max_mate_plies = value().parse().expect("--max-mate-plies")
                }
                "--node-cap" => args.node_cap = value().parse().expect("--node-cap"),
                "--per-bucket" => args.per_bucket = value().parse().expect("--per-bucket"),
                "--triage-cp" => args.triage_cp = value().parse().expect("--triage-cp"),
                "--seed" => args.seed = value().parse().expect("--seed"),
                "--opening-plies" => {
                    args.opening_plies = Some(value().parse().expect("--opening-plies"))
                }
                "--hash" => args.hash_mb = value().parse().expect("--hash"),
                "--network" => args.network = Some(PathBuf::from(value())),
                "--out" => args.out = PathBuf::from(value()),
                other => panic!("unknown flag {other}"),
            }
        }
        // A forced mate ends on the attacker's move, so its length in plies is
        // always odd; an even bound would only ever waste the last ply.
        if args.max_mate_plies.is_multiple_of(2) {
            panic!("--max-mate-plies must be odd (a forced mate is an odd number of plies)");
        }
        args
    }
}

fn main() {
    let args = Args::parse();

    let network = args.network.as_ref().map(|path| {
        let file = std::fs::File::open(path)
            .unwrap_or_else(|e| panic!("cannot open network {}: {e}", path.display()));
        let mut reader = std::io::BufReader::new(file);
        let net = Network::read(&mut reader)
            .unwrap_or_else(|e| panic!("cannot load network {}: {e}", path.display()));
        Arc::new(net)
    });

    let default_opening = OpeningConfig::default();
    let opening = OpeningConfig {
        plies: args.opening_plies.unwrap_or(default_opening.plies),
        seed: args.seed,
    };

    // Data-generation defaults resign a decisively won game early, which throws
    // away exactly the positions this tool wants: the last few moves before mate,
    // where the short forced mates live. Raising the resign threshold out of
    // reach makes won games play through to real checkmate, so those near-mate
    // positions are actually searched and can be sampled. Draw adjudication is
    // left on, since a dead-drawn game holds no mates to lose.
    let adjudication = Adjudication {
        resign_score_cp: i32::MAX,
        ..Adjudication::default()
    };

    let config = SelfPlayConfig {
        node_budget: args.nodes,
        // One worker keeps the game order, and therefore the whole run,
        // deterministic for a given seed.
        workers: 1,
        games: args.games,
        hash_size_mb: args.hash_mb,
        max_plies: 800,
        adjudication,
        opening,
        network: network.clone(),
    };

    let triage = Score::cp(args.triage_cp);
    let mut seen: HashSet<String> = HashSet::new();
    let mut found: Vec<SuitePosition> = Vec::new();
    let mut attempted = 0u64;

    eprintln!(
        "Self-play: {} games, {} nodes/move, evaluator: {}",
        args.games,
        args.nodes,
        args.network
            .as_ref()
            .map_or_else(|| "hand-crafted".to_string(), |p| p.display().to_string()),
    );

    selfplay::run(&config, |record| {
        for sample in &record.samples {
            // Speed filter only (see module docs): skip positions the self-play
            // search did not already see as a decisive win for the side to move.
            if sample.score < triage {
                continue;
            }
            let mut pos = sample.position.clone();
            attempted += 1;
            let mate_plies =
                match MateSolver::new(args.node_cap).solve(&mut pos, args.max_mate_plies) {
                    MateResult::Mate(d) => d,
                    MateResult::NoMate | MateResult::Budget => continue,
                };

            let fen = pos.to_fen();
            if !seen.insert(fen.clone()) {
                continue;
            }

            let winners = MateSolver::new(args.node_cap).winning_first_moves(&mut pos, mate_plies);
            if winners.is_empty() {
                // Only happens if the node cap was hit on this second pass; treat
                // the position as unproven rather than admit it without a move.
                seen.remove(&fen);
                continue;
            }
            let line = match MateSolver::new(args.node_cap).mate_line(&mut pos, args.max_mate_plies)
            {
                Some(line) => line,
                None => {
                    seen.remove(&fen);
                    continue;
                }
            };
            if !playout_is_mate(&mut pos, &line) {
                eprintln!("warning: proven line failed independent playout on {fen}");
                seen.remove(&fen);
                continue;
            }

            found.push(SuitePosition {
                fen,
                mate_plies,
                mate_moves: mate_plies.div_ceil(2),
                winning_first_moves: winners.iter().map(|m| m.to_uci_string()).collect(),
                line: line.iter().map(|m| m.to_uci_string()).collect(),
            });
        }
    });

    // Deterministic order, then cap each mate-distance bucket so the suite is
    // balanced across distances rather than dominated by the commonest one.
    found.sort_by(|a, b| {
        a.mate_plies
            .cmp(&b.mate_plies)
            .then_with(|| a.fen.cmp(&b.fen))
    });
    let mut per_bucket: BTreeMap<u32, usize> = BTreeMap::new();
    let mut balanced: Vec<SuitePosition> = Vec::new();
    for position in found {
        let taken = per_bucket.entry(position.mate_moves).or_insert(0);
        if *taken < args.per_bucket {
            *taken += 1;
            balanced.push(position);
        }
    }

    let counts_by_mate_moves = per_bucket.clone();
    let suite = Suite {
        description: "Rules-verified forced mates from Seaborg self-play. Each position is a \
                      game-theoretic forced mate for the side to move, proven by exhaustive \
                      minimax over the move generator independently of the engine's search."
            .to_string(),
        source: format!(
            "seaborg self-play (evaluator: {})",
            args.network
                .as_ref()
                .map_or_else(|| "hand-crafted".to_string(), |p| p.display().to_string())
        ),
        params: Params {
            games: args.games,
            self_play_nodes: args.nodes,
            max_mate_plies: args.max_mate_plies,
            solver_node_cap: args.node_cap,
            per_bucket: args.per_bucket,
            triage_cp: args.triage_cp,
            opening_seed: args.seed,
            opening_plies: config.opening.plies,
            hash_mb: args.hash_mb,
            network: args
                .network
                .as_ref()
                .map_or_else(|| "hand-crafted".to_string(), |p| p.display().to_string()),
        },
        counts_by_mate_moves,
        positions: balanced,
    };

    let json = serde_json::to_string_pretty(&suite).expect("serialize suite");
    std::fs::write(&args.out, json + "\n")
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", args.out.display()));

    eprintln!(
        "Attempted {attempted} decisive positions; wrote {} verified mates to {}",
        suite.positions.len(),
        args.out.display()
    );
    for (moves, count) in &suite.counts_by_mate_moves {
        eprintln!("  mate in {moves}: {count}");
    }
}
