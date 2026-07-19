use core::init::init_globals;
use core::position::Position;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use engine::search::{Search, Worker};
use engine::tt::Table;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const SEARCH_DEPTH: u8 = 7;

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

criterion_group!(benches, search_benchmark);
criterion_main!(benches);
