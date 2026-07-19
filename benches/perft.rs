use chess_core::init::init_globals;
use chess_core::position::Position;
use criterion::{criterion_group, criterion_main, Criterion};
use engine::perft::Perft;
use std::hint::black_box;

fn run_perft(pos: &mut Position) {
    let _res = Perft::perft(pos, 5, false, false, false);
}

fn perft_benchmark(c: &mut Criterion) {
    init_globals();

    let mut position = Position::start_pos();
    c.bench_function("perft 5", |b| {
        b.iter(|| run_perft(black_box(&mut position)));
    });
}

criterion_group!(benches, perft_benchmark);
criterion_main!(benches);
