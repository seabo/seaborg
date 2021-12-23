use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rchess::bb::Bitboard;

fn criterion_benchmark(c: &mut Criterion) {
    let bb = Bitboard::new(0x00000000C0000000);
    let bb2 = Bitboard::new(0xAB878DE7787627F8);

    c.bench_function("bsf", |b| b.iter(|| black_box(bb.bsf())));
    c.bench_function("iterate set bits", |b| {
        b.iter(|| {
            for x in bb2 {
                black_box(x);
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
