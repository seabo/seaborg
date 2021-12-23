use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rchess::mov::Move;
use rchess::position::{Position, Square};

fn criterion_benchmark(c: &mut Criterion) {
    let fen = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    let position = Position::from_fen(fen).unwrap();

    c.bench_function("build move u16", |b| {
        b.iter(|| black_box(Move::build(Square::C3, Square::D2, None, false, false)))
    });

    c.bench_function("generate pawn moves", |b| {
        b.iter(|| black_box(position.generate_moves()))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
