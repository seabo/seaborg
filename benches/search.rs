//! Search benchmarks.
//!
//! Two groups, measuring different things:
//!
//! * The **start-position depth-7** pair tracks per-node overhead and the cost of deadline
//!   checking. Its tree is tiny and almost entirely served from a warm table, which is what makes
//!   it a sensitive overhead probe and what makes it useless for anything hash-related.
//! * The **hash load** group searches trees large enough to fill the table, from an empty table
//!   each iteration. This is the group that can see a change in probe or store cost.

use chess_core::init::init_globals;
use chess_core::position::Position;
use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use engine::eval::Evaluation;
use engine::search::{Search, Worker};
use engine::tt::Table;
use std::hint::black_box;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const SEARCH_DEPTH: u8 = 7;

/// The default UCI hash size, so the figures describe the table a search actually runs against.
const HASH_LOAD_MB: usize = 16;

/// Positions and fixed depths whose trees are large enough to load the table.
///
/// Each depth is chosen so the tree reaches roughly one to two million nodes, which at 16MB drives
/// occupancy to between half and completely full — that is, into the regime where replacement runs
/// and probes miss cache. The four positions cover materially different search shapes: an opening
/// with a wide shallow tree, two dense middlegames with heavy tactical branching, and a sparse
/// endgame that reaches far deeper for the same node count.
const HASH_LOAD_POSITIONS: &[(&str, &str, u8)] = &[
    (
        "startpos depth 9",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        9,
    ),
    (
        "kiwipete depth 8",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        8,
    ),
    (
        "middlegame depth 8",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 1",
        8,
    ),
    (
        "endgame depth 11",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        11,
    ),
];

fn search_benchmark(c: &mut Criterion) {
    init_globals();

    // The representative configuration: a real UCI search under a time control always carries a
    // deadline, so this is the figure that tracks engine speed in play. The deadline is set far
    // enough out that it never fires, so the tree searched is identical to the variant below.
    {
        let stop = AtomicBool::new(false);
        let table = Table::new(16);
        let stop_time = Instant::now() + Duration::from_secs(24 * 60 * 60);
        let mut search = Search::new(Position::start_pos(), &stop, Some(stop_time), &table);
        c.bench_function("search startpos depth 7", |b| {
            b.iter(|| black_box(search.run::<Worker>(SEARCH_DEPTH)))
        });
    }

    // The same search with no deadline at all, which takes `stopping()` down a path that never
    // reads the clock. The gap between the two is the cost of deadline checking; keeping both
    // measurable is what makes a regression in that cost attributable rather than mysterious.
    {
        let stop = AtomicBool::new(false);
        let table = Table::new(16);
        let mut search = Search::new(Position::start_pos(), &stop, None, &table);
        c.bench_function("search startpos depth 7 no deadline", |b| {
            b.iter(|| black_box(search.run::<Worker>(SEARCH_DEPTH)))
        });
    }
}

/// Search representative positions to a fixed depth against a table that starts empty.
///
/// Clearing between iterations is what makes this group mean anything. Criterion re-runs the
/// closure many times, and a table left populated by the previous iteration answers almost every
/// probe with an entry deep enough to cut off, so the second and later iterations search a few
/// hundred nodes instead of a few million. Timing would then describe a search that no longer
/// exists. The clear is deliberately outside the timed region: it is a fixed linear cost that has
/// nothing to do with what this group measures, and `benches/tt.rs` already times it directly.
fn hash_load_benchmark(c: &mut Criterion) {
    init_globals();

    report_hash_load_telemetry();

    let mut group = c.benchmark_group("search hash load");
    // Each iteration searches for hundreds of milliseconds. Flat sampling runs one iteration per
    // sample rather than criterion's default linearly growing iteration counts, which for a
    // benchmark this slow is the difference between ten searches and fifty-five.
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .measurement_time(Duration::from_secs(20));

    for (name, fen, depth) in HASH_LOAD_POSITIONS {
        let pos = Position::from_fen(fen).expect("benchmark position is a valid FEN");

        group.bench_function(*name, |b| {
            b.iter_custom(|iters| {
                let stop = AtomicBool::new(false);
                let mut table = Table::new(HASH_LOAD_MB);
                let mut elapsed = Duration::ZERO;

                for _ in 0..iters {
                    table.clear();
                    let mut search = Search::new(pos.clone(), &stop, None, &table);
                    let start = Instant::now();
                    black_box(search.run::<Worker>(*depth));
                    elapsed += start.elapsed();
                }

                elapsed
            })
        });
    }

    group.finish();
}

/// Print the node count and probe outcome of one clean run of each hash-load position.
///
/// Elapsed time on its own cannot say what a change did. A faster search that visits the same
/// nodes got cheaper per node; a faster search that visits fewer nodes got better informed, and the
/// two call for completely different conclusions. Node counts here are exact and reproduce run to
/// run, so they distinguish the cases where the timings cannot.
fn report_hash_load_telemetry() {
    println!("\nsearch hash load baseline (one clean run per position, {HASH_LOAD_MB}MB table)");
    println!(
        "{:<20} {:>10} {:>10} {:>10} {:>7} {:>9}",
        "position", "nodes", "probes", "hits", "hit %", "hashfull"
    );

    for (name, fen, depth) in HASH_LOAD_POSITIONS {
        let pos = Position::from_fen(fen).expect("benchmark position is a valid FEN");
        let stop = AtomicBool::new(false);
        let table = Table::new(HASH_LOAD_MB);
        let mut search = Search::new(pos, &stop, None, &table);
        search.run::<Worker>(*depth);

        let trace = search.trace();
        let probes = trace.hash_probes();
        let hits = trace.hash_hits();
        println!(
            "{:<20} {:>10} {:>10} {:>10} {:>6.1}% {:>9}",
            name,
            trace.all_nodes_visited(),
            probes,
            hits,
            hits as f64 / probes as f64 * 100.0,
            table.hashfull()
        );
    }
    println!();
}

/// Measure one static evaluation.
///
/// This figure exists to bound a specific question: what could be saved by storing a position's
/// static evaluation in its transposition-table entry rather than recomputing it. That saving can
/// never exceed one evaluation per node that probes successfully, so an evaluation cost per node,
/// set against the search's own cost per node in the hash-load group above, is the ceiling on any
/// such scheme. Recording it makes the trade arithmetic rather than folklore, and makes it
/// re-checkable the moment the evaluation stops being material-only.
fn evaluation_benchmark(c: &mut Criterion) {
    init_globals();

    let mut group = c.benchmark_group("static evaluation");

    for (name, fen, _) in HASH_LOAD_POSITIONS {
        let pos = Position::from_fen(fen).expect("benchmark position is a valid FEN");
        group.bench_function(
            name.split(" depth")
                .next()
                .expect("name has a depth suffix"),
            |b| b.iter(|| black_box(black_box(&pos).material_eval())),
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    search_benchmark,
    hash_load_benchmark,
    evaluation_benchmark
);
criterion_main!(benches);
