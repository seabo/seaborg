use core::position::Position;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn gen_moves(fen: &str) {
    let position = Position::from_fen(fen).unwrap();
    position.generate_moves();
}

fn criterion_benchmark(c: &mut Criterion) {
    let fen = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    c.bench_function("generate moves", |b| b.iter(|| gen_moves(black_box(fen))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
