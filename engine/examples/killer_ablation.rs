//! Fixed-depth killer-table ablation harness.
//!
//! This is a measurement tool, not engine code. It runs a single-threaded search to a fixed depth on
//! a small fixed set of positions, from a fresh transposition table each time, and reports the node
//! count and throughput. Because the killer width is a compile-time constant, the disabled / one-slot
//! / two-slot comparison is run by building this example three times with `KILLER_SLOTS` set to `0`,
//! `1` and `2` in `engine/src/search.rs`. The active width is printed so each run is self-labelling.
//!
//! Node counts here are deterministic: a fixed depth with no time or node limit visits the same tree
//! every run on a given build, so a change in node count between builds is attributable to the killer
//! policy alone. Throughput is wall-clock and therefore noisy; take it only as a coarse cost signal
//! and run on an otherwise idle machine.
//!
//! # Usage
//!
//! ```text
//! # edit KILLER_SLOTS in engine/src/search.rs to 0, then:
//! RUSTFLAGS="-C target-cpu=native" cargo run --release -p engine --example killer_ablation
//! # repeat with KILLER_SLOTS = 1 and = 2
//! ```

use core::init::init_globals;
use core::position::Position;

use engine::search::{Search, KILLER_SLOTS};
use engine::tt::Table;

use std::sync::atomic::AtomicBool;

/// The default UCI hash size, so the figures describe the table a real search runs against.
const HASH_MB: usize = 16;

/// Positions and fixed depths spanning materially different search shapes: a wide shallow opening,
/// two dense tactical middlegames, and a sparse endgame that reaches far deeper for a similar node
/// count. Depths are chosen to keep each single search to a few seconds.
const POSITIONS: &[(&str, &str, u8)] = &[
    (
        "startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        11,
    ),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        10,
    ),
    (
        "middlegame",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 1",
        10,
    ),
    (
        "endgame",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        14,
    ),
];

fn main() {
    init_globals();

    println!("killer ablation | KILLER_SLOTS = {KILLER_SLOTS}");
    println!(
        "{:<12} {:>6} {:>14} {:>14} {:>18}",
        "position", "depth", "nodes", "nps", "killer cutoffs/slot"
    );

    let mut total_nodes: u64 = 0;
    for &(label, fen, depth) in POSITIONS {
        let pos = Position::from_fen(fen).expect("example FEN is valid");
        let flag = AtomicBool::new(false);
        let table = Table::new(HASH_MB);
        let mut search = Search::new(pos, &flag, None, &table);

        search
            .run::<engine::search::Worker>(depth)
            .expect("a fixed-depth search returns a result");

        let trace = search.trace();
        let nodes = trace.all_nodes_visited();
        let nps = trace.nps().unwrap_or(0);
        let attempts = trace.killer_attempts();
        let cutoffs = trace.killer_cutoffs();

        total_nodes += nodes as u64;

        let per_slot: Vec<String> = attempts
            .iter()
            .zip(cutoffs.iter())
            .map(|(&a, &c)| format!("{c}/{a}"))
            .collect();

        println!(
            "{label:<12} {depth:>6} {nodes:>14} {nps:>14} {:>18}",
            per_slot.join(" ")
        );
    }

    println!("total nodes: {total_nodes}");
}
