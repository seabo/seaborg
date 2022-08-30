use core::position::Square;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("square from idx", |b| b.iter(|| black_box(Square(34))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
