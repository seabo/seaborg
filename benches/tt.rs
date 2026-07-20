//! Transposition-table lifecycle and hot-path benchmarks.
//!
//! Four things are measured, because they fail in different ways:
//!
//! * **Construction and clearing** are the lifecycle costs. They are linear in table size and are
//!   paid at `setoption Hash` and `ucinewgame` respectively, off the search hot path. Clearing is
//!   physical rather than a generation bump, so its cost is real and is recorded rather than
//!   assumed negligible.
//! * **Probe and store** are the hot path, measured on a table far larger than cache so that the
//!   figure includes the cache miss a real search pays.
//! * **A multi-worker mixed load** is what a Lazy SMP search actually does. Comparing its
//!   throughput against the single-threaded figure is what would expose false sharing or
//!   replacement contention between workers.

use chess::mov::Move;
use criterion::{criterion_group, criterion_main, Criterion};
use engine::score::Score;
use engine::tt::{Bound, Table};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

/// Large enough that probes miss cache, which is the regime a real search runs in.
const HOT_PATH_MB: usize = 64;
/// Large enough for the linear lifecycle costs to be measurable above timer noise.
const LIFECYCLE_MB: usize = 256;

/// A cheap key sequence with no relationship to the cluster index, so successive probes land in
/// unrelated cache lines exactly as a search's keys do.
#[inline(always)]
fn key(i: u64) -> u64 {
    i.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1
}

fn lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("tt lifecycle");
    // Each sample allocates or walks 256MB, so the default sample count would take minutes.
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(10));

    group.bench_function("construct 256MB", |b| {
        b.iter(|| black_box(Table::new(LIFECYCLE_MB)))
    });

    group.bench_function("clear 256MB", |b| {
        let mut table = Table::new(LIFECYCLE_MB);
        for i in 0..1_000_000 {
            table.store(key(i), Score::cp(1), None, 1, Bound::Exact, &Move::null());
        }
        b.iter(|| table.clear())
    });

    group.finish();
}

fn hot_path(c: &mut Criterion) {
    let table = Table::new(HOT_PATH_MB);
    let entries = table.capacity_entries() as u64;

    // Fill the table, so probes exercise a populated cluster scan rather than an early exit on
    // four empty slots.
    for i in 0..entries {
        table.store(
            key(i),
            Score::cp(1),
            None,
            (i % 64) as u8,
            Bound::Exact,
            &Move::null(),
        );
    }

    let mut i = 0_u64;
    c.bench_function("tt probe hit", |b| {
        b.iter(|| {
            i = i.wrapping_add(1);
            black_box(table.probe(key(i % entries)))
        })
    });

    let mut i = 0_u64;
    c.bench_function("tt probe miss", |b| {
        b.iter(|| {
            i = i.wrapping_add(1);
            // Keys the table has never seen: a full four-slot scan with no match, which is the
            // worst case for the probe.
            black_box(table.probe(key(i).wrapping_add(1) ^ 0xffff_0000_0000_0000))
        })
    });

    let mut i = 0_u64;
    c.bench_function("tt store", |b| {
        b.iter(|| {
            i = i.wrapping_add(1);
            table.store(
                black_box(key(i)),
                Score::cp(7),
                None,
                (i % 64) as u8,
                Bound::Exact,
                &Move::null(),
            )
        })
    });
}

fn multi_worker(c: &mut Criterion) {
    const WORKERS: u64 = 4;
    const OPS_PER_WORKER: u64 = 250_000;

    let mut group = c.benchmark_group("tt multi worker");
    group.sample_size(20);

    for workers in [1_u64, WORKERS] {
        // The same total work in every configuration, so the figures are directly comparable:
        // if throughput does not hold up as workers are added, the table is the bottleneck.
        let ops = OPS_PER_WORKER * WORKERS / workers;

        group.bench_function(format!("{workers} workers, mixed probe/store"), |b| {
            let table = Arc::new(Table::new(HOT_PATH_MB));
            let entries = table.capacity_entries() as u64;

            b.iter(|| {
                std::thread::scope(|scope| {
                    for worker in 0..workers {
                        let table = Arc::clone(&table);
                        scope.spawn(move || {
                            for i in 0..ops {
                                // Workers share one key space with no partitioning, so they
                                // contend for the same clusters exactly as Lazy SMP workers do.
                                let k = key((i * WORKERS + worker) % entries);
                                if i % 4 == 0 {
                                    table.store(
                                        k,
                                        Score::cp(3),
                                        None,
                                        (i % 64) as u8,
                                        Bound::Exact,
                                        &Move::null(),
                                    );
                                } else {
                                    black_box(table.probe(k));
                                }
                            }
                        });
                    }
                })
            })
        });
    }

    group.finish();
}

criterion_group!(benches, lifecycle, hot_path, multi_worker);
criterion_main!(benches);
