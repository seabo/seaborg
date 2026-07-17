# Performance benchmark baseline

The performance baseline for move generation is commit
`d7366ab0790154a8626ff53f62011917f96730a3`. It was measured with Criterion
after competing test processes had finished and the machine had reached a
sustained idle period.

## Baseline results

| Benchmark | Baseline | Criterion 95% interval |
| --- | ---: | ---: |
| `generate moves` | 184.60 ns | 183.71–185.76 ns |
| `perft 5` | 21.402 ms | 21.332–21.496 ms |
| Start-position perft throughput | 227.34 million nodes/s | — |

The measurements were taken on an Apple M3 Pro with 6 performance and 6
efficiency cores, using `rustc 1.97.1` and `cargo 1.97.1`. Perft used the
standard starting position at depth 5 (4,865,609 nodes). Move generation used
the position embedded in `benches/movegen.rs`.

Run the same benchmarks with:

```sh
cargo bench --bench perft --bench movegen
```

For routine regression checks on the same hardware and toolchain, investigate
results slower than the baseline by 5% or more:

- `generate moves`: greater than 193.83 ns
- `perft 5`: greater than 22.472 ms

Small differences inside Criterion's confidence intervals should be treated as
measurement noise. Run benchmarks on an otherwise idle machine, and compare
like-for-like hardware and toolchains.

This baseline is a regression target, not a permanent historical constant. If
an intentional engine change produces a repeatable performance improvement,
update this document to the improved measurements and record the commit,
hardware, and toolchain used. Do not lower the baseline from a single noisy run.
