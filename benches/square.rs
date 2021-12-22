use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rchess::position::Square;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("square from idx", |b| {
        b.iter(|| black_box(Square::from_idx(34)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
