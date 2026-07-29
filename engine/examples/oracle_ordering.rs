//! Oracle move-ordering ceiling harness.
//!
//! This is a measurement tool, not engine code, and it compiles only under the `oracle` feature. It
//! answers the single question the selectivity investigation hinges on: of the gap between our
//! effective branching factor and the minimal-tree frontier, how much is pure move-ordering waste —
//! recoverable with no soundness cost by simply searching the right move first — versus how much is
//! genuinely eval- and pruning-limited?
//!
//! For each position it runs [`SearchEngine::oracle_profile`], which searches once to a fixed depth
//! from a cold table (the ordinary search — `EBF_real`), then re-searches the same position to the
//! same depth from a cold table with every node seeded with the true best move that first search
//! proved for it (`EBF_oracle`). Only move ordering differs between the two, so `EBF_real - EBF_oracle`
//! estimates what ordering alone can buy and `EBF_oracle - frontier` the part that ordering cannot
//! reach. Both searches evaluate through the engine's built-in network, so `EBF_real` is comparable to
//! the effective branching factor an ordinary search reports.
//!
//! The oracle forces its move to the very front and so searches it at full depth, which interacts with
//! late-move reduction: at all-node-heavy quiet positions, forcing a non-cutting move ahead of the
//! reduced tail can add work rather than save it, so `EBF_oracle` is not a strict lower bound. It is a
//! conservative "best-first under the engine's real reductions" figure — the true free-ordering
//! headroom is at least what it reports. The effect is the opposite and large at cut-node-heavy
//! tactical positions, where forcing the refutation first roughly halves the tree.
//!
//! Node counts are deterministic (a fixed depth with no clock visits the same tree every run on a
//! given build), so the reported branching factors are exact and reproducible.
//!
//! # Usage
//!
//! ```text
//! RUSTFLAGS="-C target-cpu=native" cargo run --release -p engine --features oracle \
//!     --example oracle_ordering -- tools/diag/bench-positions.epd 14 1 64
//! ```
//!
//! Arguments (all optional, in order): EPD suite path, fixed depth, number of oracle passes,
//! transposition-table size in MB. Oracle passes must be at least 1: 1 (the default) is the
//! real→oracle two-pass this measurement reports; a higher value adds replay passes that chase the
//! fixpoint.

use chess::init::init_globals;
use chess::position::Position;

use engine::search::SearchEngine;

use std::fs;

/// The published minimal-tree reference the split is measured against: roughly the effective
/// branching factor a frontier engine reaches on this kind of suite. It is a comparison landmark, not
/// a target — Seaborg's conclusions come from its own `EBF_real`/`EBF_oracle` figures.
const FRONTIER_EBF: f64 = 2.01;

fn main() {
    let mut args = std::env::args().skip(1);
    let suite = args
        .next()
        .unwrap_or_else(|| "tools/diag/bench-positions.epd".to_string());
    let depth: u8 = args
        .next()
        .map_or(14, |a| a.parse().expect("depth must be a positive integer"));
    let iterations: usize = args
        .next()
        .map_or(1, |a| a.parse().expect("iterations must be an integer"));
    assert!(
        iterations >= 1,
        "oracle passes must be at least 1: one oracle pass is needed to compare against the \
         reference search"
    );
    let hash_mb: usize = args
        .next()
        .map_or(64, |a| a.parse().expect("hash size must be an integer"));

    init_globals();

    let positions = load_positions(&suite);
    assert!(!positions.is_empty(), "no positions loaded from {suite}");

    let mut engine = SearchEngine::new(hash_mb);

    println!(
        "oracle-ordering ceiling | suite {suite} ({} positions) | depth {depth} | \
         {iterations} replay pass(es) | hash {hash_mb} MB",
        positions.len()
    );
    println!(
        "{:<22} {:>10} {:>12} {:>12} {:>10}",
        "position", "EBF_real", "nodes_real", "nodes_oracle", "EBF_orac"
    );

    // Accumulate the natural logs so the aggregate is a geometric mean, matching the re-baseline's
    // convention: EBF is a multiplicative per-ply quantity, so a geometric mean is the meaningful
    // average across positions of different depths and shapes.
    let mut ln_real_sum = 0.0f64;
    let mut ln_oracle_sum = 0.0f64;
    // Converged oracle EBF (the last replay pass) tracked separately, to show whether one oracle pass
    // already reaches the fixpoint.
    let mut ln_converged_sum = 0.0f64;
    // Arithmetic means too: the recorded fixed-depth baseline is an arithmetic mean over the suite,
    // so `EBF_real` here can be checked against it directly.
    let mut real_sum = 0.0f64;
    let mut oracle_sum = 0.0f64;
    // Geometric mean of the per-position oracle/real node ratio: node counts are a far more sensitive
    // readout than EBF, which compresses a halving of the tree at depth 14 into a small decimal.
    let mut ln_node_ratio_sum = 0.0f64;

    for (fen, label) in &positions {
        let pos = Position::from_fen(fen).expect("suite FEN is valid");
        let passes = engine.oracle_profile(pos, depth, iterations);

        let real = &passes[0];
        // With `iterations >= 1` there is at least one oracle pass; index 1 is the headline two-pass
        // oracle figure and the last pass is the converged one.
        let oracle = &passes[1];
        let converged = passes.last().expect("at least the reference pass exists");

        ln_real_sum += f64::from(real.ebf).ln();
        ln_oracle_sum += f64::from(oracle.ebf).ln();
        ln_converged_sum += f64::from(converged.ebf).ln();
        real_sum += f64::from(real.ebf);
        oracle_sum += f64::from(oracle.ebf);
        ln_node_ratio_sum += (oracle.all_nodes as f64 / real.all_nodes as f64).ln();

        println!(
            "{label:<22} {:>10.3} {:>12} {:>12} {:>10.3}",
            real.ebf, real.all_nodes, oracle.all_nodes, oracle.ebf
        );
    }

    let n = positions.len() as f64;
    let ebf_real = (ln_real_sum / n).exp();
    let ebf_oracle = (ln_oracle_sum / n).exp();
    let ebf_converged = (ln_converged_sum / n).exp();
    let node_ratio = (ln_node_ratio_sum / n).exp();

    println!();
    println!(
        "== aggregate over {} positions, depth {depth} ==",
        positions.len()
    );
    println!(
        "EBF_real     geomean {ebf_real:.3}   mean {:.3}",
        real_sum / n
    );
    println!(
        "EBF_oracle   geomean {ebf_oracle:.3}   mean {:.3}  (1 pass)",
        oracle_sum / n
    );
    if iterations > 1 {
        println!(
            "EBF_oracle   geomean {ebf_converged:.3}          (converged, {iterations} passes)"
        );
    }
    println!("frontier reference   {FRONTIER_EBF:.3}");
    println!("oracle/real nodes    {node_ratio:.3} (geomean per-position node ratio)");
    println!();
    println!(
        "free ordering headroom  EBF_real - EBF_oracle = {:.3}",
        ebf_real - ebf_oracle
    );
    println!(
        "eval/pruning-limited    EBF_oracle - frontier = {:.3}",
        ebf_oracle - FRONTIER_EBF
    );
    let total_gap = ebf_real - FRONTIER_EBF;
    if total_gap > 0.0 {
        println!(
            "ordering share of gap   {:.1}% of (EBF_real - frontier)",
            (ebf_real - ebf_oracle) / total_gap * 100.0
        );
    }
}

/// Read `(fen, label)` pairs from an EPD/bench suite, keeping only the six FEN fields and taking the
/// label from a trailing `;` comment. Blank lines and `#` comments are skipped. Mirrors the loader
/// the Python diagnostics use so both read the same suite identically.
fn load_positions(path: &str) -> Vec<(String, String)> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (body, label) = match line.split_once(';') {
            Some((body, rest)) => (body.trim(), rest.trim().to_string()),
            None => (line, String::new()),
        };
        let fields: Vec<&str> = body.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        out.push((fields[..6].join(" "), label));
    }
    out
}
