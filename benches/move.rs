use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rchess::mov::{Move, MoveStruct};
use rchess::position::Square;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("build move", |b| {
        b.iter(|| black_box(Move::build(Square::C3, Square::D2, None, false, false)))
    });

    c.bench_function("build move u16", |b| {
        b.iter(|| {
            black_box(MoveStruct::build(
                Square::C3,
                Square::D2,
                None,
                false,
                false,
            ))
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
