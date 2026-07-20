use chess::init::init_globals;
use chess::mono_traits::{All, Legal};
use chess::movelist::BasicMoveList;
use chess::position::Position;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn gen_moves(position: &Position) -> BasicMoveList {
    position.generate::<BasicMoveList, All, Legal>()
}

fn criterion_benchmark(c: &mut Criterion) {
    init_globals();
    let fen = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    let position = Position::from_fen(fen).unwrap();
    c.bench_function("generate moves", |b| {
        b.iter(|| gen_moves(black_box(&position)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
