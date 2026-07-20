//! Offline quiescence-reachability explorer.
//!
//! This is an investigation tool, not engine code. It exists to supply structural evidence about
//! the ply-1 quiescence tree that the abort-suppressed window (`Search::min_search_complete`) must
//! run to completion before a UCI `stop` can take effect. It lives in `examples/` rather than
//! `engine/src` because it is measurement scaffolding that should not ship inside the engine.
//!
//! # Keeping this model honest
//!
//! Every figure this tool reports is meaningful only while its move selection still matches
//! `Search::quiesce` / `Search::quiesce_evasions`. That correspondence is a maintained invariant,
//! not an incidental resemblance: if you change what quiescence expands, update the model below to
//! match. Nothing enforces this — the example keeps compiling and keeps printing plausible numbers
//! once it has drifted, so a stale model is silently wrong rather than loudly broken.
//!
//! # What it models
//!
//! It replicates the *move selection* of `Search::quiesce` / `Search::quiesce_evasions` exactly:
//!
//! - A q-node that is **not** in check expands queen promotions and captures only. This mirrors
//!   `QMoveLoader::load_promotions` + `load_captures`; `QMoveLoader::load_quiets` is gated on
//!   `in_check()` and is unreachable from `quiesce`, because `quiesce` diverts every in-check node
//!   to `quiesce_evasions` before the `OrderedMoves` loop runs.
//! - A q-node that **is** in check expands every legal move, quiet moves included. This mirrors
//!   `quiesce_evasions`, and is the only way a quiet move ever enters the quiescence tree.
//! - The only non-cap terminations are `quiesce` Step 1: threefold repetition and
//!   `half_move_clock() >= 50`.
//!
//! It deliberately omits stand-pat, the TT cutoff, and alpha-beta. Those only ever *prune*, so the
//! tree measured here is a sound **upper bound** on the ply-1 q-tree the engine can visit. A small
//! bound here is therefore a real bound on the engine; a large bound here is only a bound on
//! reachability, not a claim that the engine visits that many nodes.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p engine --example qtree_reachability -- corpus [node_cap]
//! cargo run --release -p engine --example qtree_reachability -- wac [node_cap]
//! cargo run --release -p engine --example qtree_reachability -- sweep <positions> <seed> [node_cap]
//! ```
//!
//! `wac` and `sweep` are the systematic adversarial search: every position is ranked by the depth
//! and size of its reachable ply-1 q-tree and by `max_quiet_check_chain`, the run of consecutive
//! quiet check evasions that drives quiescence deepest.

use chess::mono_traits::{All, Captures, Legal, QueenPromotions};
use chess::mov::Move;
use chess::movelist::BasicMoveList;
use chess::position::Position;

/// Ply cap applied to the modelled q-tree, counted from the depth-1 child.
const MAX_Q_PLY: u32 = 64;

/// Default node cap. Without alpha-beta the modelled tree can be far larger than the engine's, so
/// exploration is truncated rather than allowed to run unbounded. Truncation is always reported.
const DEFAULT_MAX_Q_NODES: u64 = 20_000_000;

/// Structural metrics for one modelled q-tree.
#[derive(Default, Clone, Copy)]
struct QStats {
    /// Total modelled quiescence nodes visited.
    q_nodes: u64,
    /// Deepest ply reached, counted from the depth-1 child.
    max_q_ply: u32,
    /// Longest run of consecutive in-check q-nodes left by a *quiet* (non-capture, non-promotion)
    /// evasion. Each such evasion extends the tree by a ply without resolving the position, so
    /// this is the mechanism by which quiescence runs deep.
    max_quiet_check_chain: u32,
    /// Whether either cap stopped exploration, making the other figures lower bounds.
    truncated: bool,
}

impl QStats {
    fn merge(&mut self, other: &QStats) {
        self.q_nodes += other.q_nodes;
        self.max_q_ply = self.max_q_ply.max(other.max_q_ply);
        self.max_quiet_check_chain = self.max_quiet_check_chain.max(other.max_quiet_check_chain);
        self.truncated |= other.truncated;
    }
}

/// Expand one modelled q-node.
///
/// `ply` counts from the depth-1 child, and `quiet_check_chain` is the number of consecutive quiet
/// check evasions that led here.
fn explore(pos: &mut Position, ply: u32, quiet_check_chain: u32, cap: u64, stats: &mut QStats) {
    stats.q_nodes += 1;
    stats.max_q_ply = stats.max_q_ply.max(ply);
    stats.max_quiet_check_chain = stats.max_quiet_check_chain.max(quiet_check_chain);

    if stats.q_nodes >= cap {
        stats.truncated = true;
        return;
    }

    // `quiesce` Step 1. Quiet check evasions can repeat positions, so this runs before any
    // expansion.
    if pos.in_threefold() || pos.half_move_clock() >= 50 {
        return;
    }

    if ply >= MAX_Q_PLY {
        stats.truncated = true;
        return;
    }

    let in_check = pos.in_check();

    // `quiesce_evasions` for in-check nodes, `QMoveLoader` otherwise.
    let moves: Vec<Move> = if in_check {
        pos.generate::<BasicMoveList, All, Legal>()
            .into_iter()
            .copied()
            .collect()
    } else {
        let promos = pos.generate::<BasicMoveList, QueenPromotions, Legal>();
        let captures = pos.generate::<BasicMoveList, Captures, Legal>();
        promos.into_iter().chain(&captures).copied().collect()
    };

    for mov in &moves {
        // A quiet check evasion is the only edge that extends the chain: it neither captures nor
        // promotes, so it makes no material progress toward termination.
        let next_chain = if in_check && !mov.is_capture() && !mov.is_promo() {
            quiet_check_chain + 1
        } else {
            0
        };

        pos.make_move(mov);
        explore(pos, ply + 1, next_chain, cap, stats);
        pos.unmake_move();

        if stats.truncated {
            return;
        }
    }
}

/// Model the full ply-1 quiescence work for `pos`: the union of the q-trees rooted at each
/// depth-1 root child, which is exactly what the depth-1 iteration must finish before
/// `min_search_complete` is set and a pending `stop` can take effect.
fn ply_one_qtree(pos: &mut Position, cap: u64) -> QStats {
    let root_moves: Vec<Move> = pos
        .generate::<BasicMoveList, All, Legal>()
        .into_iter()
        .copied()
        .collect();

    let mut total = QStats::default();
    for mov in &root_moves {
        let mut child = QStats::default();
        pos.make_move(mov);
        explore(pos, 1, 0, cap, &mut child);
        pos.unmake_move();
        total.merge(&child);
        if total.truncated {
            break;
        }
    }
    total
}

/// The corpus measured by `tools/stop_latency_probe.rb`, plus positions constructed specifically to
/// drive the quiet check-evasion mechanism.
fn corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        // Probe corpus.
        (
            "startpos",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "kiwipete_dense",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        ),
        (
            "perft_checks_promotions",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        ),
        (
            "dense_tactics",
            "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17",
        ),
        (
            "many_captures",
            "rnb1kb1r/p4p2/1qp1pn2/1p2N2p/2p1P1p1/2N3B1/PPQ1BPPP/3RK2R w Kkq - 0 1",
        ),
        (
            "capture_chain",
            "k3nrn1/4b3/3q1p1R/8/4N1NB/2Q5/5R2/K7 w - - 0 1",
        ),
        ("in_check_quiet_evasions", "k3r3/8/8/8/8/8/8/4K3 w - - 0 1"),
        (
            "mate_tactics_1",
            "r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25",
        ),
        (
            "mate_tactics_2",
            "6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34",
        ),
        ("check_heavy", "3kB3/5K2/7p/3p4/3pn3/4NN2/8/1b4B1 w - - 0 1"),
        // Constructed adversaries for the quiet check-evasion chain. Each puts the side to move in
        // check with quiet evasions available, in a setting where the evading side can give check
        // back, which is what a self-sustaining quiet chain requires.
        (
            "adv_mutual_check_battery",
            "4r2k/4q3/8/8/8/8/4Q3/4R2K w - - 0 1",
        ),
        (
            "adv_perpetual_check_queens",
            "6k1/5ppp/8/8/8/8/5PPP/1Q4K1 w - - 0 1",
        ),
        (
            "adv_discovered_check_battery",
            "3rk3/8/8/8/8/8/3B4/3RK3 w - - 0 1",
        ),
        ("adv_rook_ladder_checks", "7k/8/8/8/8/8/R7/1R5K w - - 0 1"),
        (
            "adv_open_kings_many_evasions",
            "4k3/8/8/3q4/3Q4/8/8/4K3 b - - 0 1",
        ),
        (
            "adv_knight_check_net",
            "4k3/8/3N1N2/8/8/3n1n2/8/4K3 b - - 0 1",
        ),
    ]
}

/// Deterministic xorshift64* so sweeps are reproducible from a seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Load the 300-position Win At Chess tactical suite. EPD records carry only four FEN fields, so
/// the halfmove/fullmove counters are appended.
fn wac_positions() -> Vec<(String, String)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../suites/wac.epd");
    let raw = std::fs::read_to_string(path).expect("wac.epd must be readable");

    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let fen = format!(
                "{} {} {} {} 0 1",
                fields[0], fields[1], fields[2], fields[3]
            );
            let id = line
                .split("id \"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap_or("unknown")
                .to_string();
            (id, fen)
        })
        .collect()
}

/// Systematically search for adversarial positions by random play from the start position,
/// measuring the modelled ply-1 q-tree of every position reached.
/// Print the distribution of `max_quiet_check_chain`, the consecutive quiet check-evasion run
/// length that a ply cap on check extensions would bound, were one added.
fn print_chain_histogram(chains: impl Iterator<Item = u32>) {
    let mut histogram = [0usize; (MAX_Q_PLY as usize) + 1];
    for chain in chains {
        histogram[chain as usize] += 1;
    }
    println!();
    println!("{:>11}  {:>9}", "quiet_chain", "positions");
    for (len, count) in histogram.iter().enumerate() {
        if *count > 0 {
            println!("{len:>11}  {count:>9}");
        }
    }
}

fn sweep(count: usize, seed: u64, cap: u64) {
    let mut rng = Rng(seed);
    let mut worst_ply: Vec<(u32, u32, u64, String)> = Vec::new();
    let mut sampled = 0usize;
    let mut deep_chain_positions = 0usize;

    while sampled < count {
        let mut pos = Position::start_pos();
        let plies = 4 + rng.below(60);

        for _ in 0..plies {
            let moves: Vec<Move> = pos
                .generate::<BasicMoveList, All, Legal>()
                .into_iter()
                .copied()
                .collect();
            if moves.is_empty() {
                break;
            }
            let pick = rng.below(moves.len());
            pos.make_move(&moves[pick]);
        }

        let legal = pos.generate::<BasicMoveList, All, Legal>();
        if legal.is_empty() {
            continue;
        }

        let fen = pos.to_fen();
        let stats = ply_one_qtree(&mut pos, cap);
        sampled += 1;

        if stats.max_quiet_check_chain >= 2 {
            deep_chain_positions += 1;
        }
        worst_ply.push((
            stats.max_q_ply,
            stats.max_quiet_check_chain,
            stats.q_nodes,
            fen,
        ));
    }

    worst_ply.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));

    println!("sweep: {sampled} positions, seed {seed}, node cap {cap}");
    println!("positions with a quiet check chain >= 2: {deep_chain_positions}");
    print_chain_histogram(worst_ply.iter().map(|r| r.1));
    println!();
    println!(
        "{:>7}  {:>11}  {:>12}  fen",
        "max_ply", "quiet_chain", "q_nodes"
    );
    for (ply, chain, nodes, fen) in worst_ply.iter().take(15) {
        println!("{ply:>7}  {chain:>11}  {nodes:>12}  {fen}");
    }
}

fn run_corpus(cap: u64) {
    println!("node cap: {cap}");
    println!(
        "{:>30}  {:>7}  {:>11}  {:>12}  truncated",
        "position", "max_ply", "quiet_chain", "q_nodes"
    );
    for (name, fen) in corpus() {
        let mut pos = Position::from_fen(fen).expect("corpus FEN must parse");
        let stats = ply_one_qtree(&mut pos, cap);
        println!(
            "{:>30}  {:>7}  {:>11}  {:>12}  {}",
            name, stats.max_q_ply, stats.max_quiet_check_chain, stats.q_nodes, stats.truncated
        );
    }
}

fn run_wac(cap: u64) {
    let positions = wac_positions();
    let mut rows: Vec<(u32, u32, u64, bool, String, String)> = Vec::new();

    for (id, fen) in &positions {
        let mut pos = Position::from_fen(fen).expect("wac FEN must parse");
        let stats = ply_one_qtree(&mut pos, cap);
        rows.push((
            stats.max_q_ply,
            stats.max_quiet_check_chain,
            stats.q_nodes,
            stats.truncated,
            id.clone(),
            fen.clone(),
        ));
    }

    let truncated = rows.iter().filter(|r| r.3).count();
    let max_chain = rows.iter().map(|r| r.1).max().unwrap_or(0);
    println!(
        "wac.epd: {} positions, node cap {cap}, {truncated} truncated, max quiet chain {max_chain}",
        rows.len()
    );
    print_chain_histogram(rows.iter().map(|r| r.1));

    rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    println!();
    println!(
        "{:>7}  {:>11}  {:>12}  {:>10}  {:>9}  fen",
        "max_ply", "quiet_chain", "q_nodes", "truncated", "id"
    );
    for (ply, chain, nodes, trunc, id, fen) in rows.iter().take(15) {
        println!("{ply:>7}  {chain:>11}  {nodes:>12}  {trunc:>10}  {id:>9}  {fen}");
    }
}

fn arg(n: usize) -> Option<String> {
    std::env::args().nth(n)
}

fn main() {
    let mode = arg(1).unwrap_or_else(|| "corpus".into());
    match mode.as_str() {
        "corpus" => {
            let cap = arg(2)
                .and_then(|a| a.parse().ok())
                .unwrap_or(DEFAULT_MAX_Q_NODES);
            run_corpus(cap);
        }
        "wac" => {
            let cap = arg(2)
                .and_then(|a| a.parse().ok())
                .unwrap_or(DEFAULT_MAX_Q_NODES);
            run_wac(cap);
        }
        "sweep" => {
            let count = arg(2).and_then(|a| a.parse().ok()).unwrap_or(2_000);
            let seed = arg(3).and_then(|a| a.parse().ok()).unwrap_or(0x5EA_B065);
            let cap = arg(4)
                .and_then(|a| a.parse().ok())
                .unwrap_or(DEFAULT_MAX_Q_NODES);
            sweep(count, seed, cap);
        }
        other => {
            eprintln!("unknown mode {other}; expected `corpus`, `wac` or `sweep`");
            std::process::exit(2);
        }
    }
}
