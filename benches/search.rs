use core::init::init_globals;
use core::position::Position;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use engine::search::{Search, Worker};
use engine::tt::Table;
use std::sync::atomic::AtomicBool;

const SEARCH_DEPTH: u8 = 7;

fn search_benchmark(c: &mut Criterion) {
    init_globals();

    let stop = AtomicBool::new(false);
    let table = Table::new(16);
    let mut search = Search::new(Position::start_pos(), &stop, None, &table);

    c.bench_function("search startpos depth 7", |b| {
        b.iter(|| black_box(search.run::<Worker>(SEARCH_DEPTH)))
    });
}

criterion_group!(benches, search_benchmark);
criterion_main!(benches);
