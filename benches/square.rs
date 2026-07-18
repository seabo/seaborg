use core::position::Square;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    // This previously benchmarked `Square(34)`, constructing a square straight
    // from a raw index. That constructor is now `pub(crate)`, so the only
    // public way to build a square from coordinates is `from_rank_file`, which
    // bounds-checks its inputs. Index 34 is rank 4, file 2.
    c.bench_function("square from rank and file", |b| {
        b.iter(|| black_box(Square::from_rank_file(black_box(4), black_box(2))))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
