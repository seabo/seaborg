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

## Search baseline

The search baseline is commit `946091b` (TASK-41), the commit that introduced the
two-configuration harness these figures come from. `benches/search.rs` measures
the start position at depth 7 in two configurations, both searching an identical
579-node tree:

| Benchmark | Baseline | Derived NPS |
| --- | ---: | ---: |
| `search startpos depth 7` | 40.25 µs | 14.39 million nodes/s |
| `search startpos depth 7 no deadline` | 39.73 µs | 14.57 million nodes/s |

The first configuration carries a deadline set 24 hours out, so it never fires
but does exercise the deadline check on every node. It is the representative
figure: a real UCI search under a time control always carries a deadline. The
second removes the deadline entirely, taking `stopping()` down a path that never
reads the clock. **The gap between the two is the cost of deadline checking.**
Keeping both measurable is what makes a regression in that cost attributable.

The measurements were taken on an Apple M3 Pro with 6 performance and 6
efficiency cores, using `rustc 1.97.1` and `cargo 1.97.1` — the same hardware and
toolchain as the move-generation baseline above.

Investigate results slower than the baseline by 5% or more:

- `search startpos depth 7`: greater than 42.26 µs
- `search startpos depth 7 no deadline`: greater than 41.72 µs

Watch the *gap* as well as the absolute figures. It is currently about 0.5 µs
(roughly 0.9 ns per node). A gap that widens back toward 10 µs means the clock
read has escaped its throttle.

### How the search figures got here

| Commit | No deadline | With deadline | Deadline cost |
| --- | ---: | ---: | ---: |
| `ebf4289` (pre-TASK-41 base) | 39.25 µs | 49.45 µs | 10.20 µs |
| `22a2512` (master, TASK-45/46) | 40.43 µs | 49.59 µs | 9.16 µs |
| `946091b` (TASK-41 throttle) | 39.73 µs | 40.25 µs | 0.52 µs |

Measured round-robin across three worktrees over three rounds, taking the
minimum per configuration; run-to-run drift on this machine is roughly 3%, which
is larger than several of the differences above, so single runs are not
trustworthy at this resolution.

Neither earlier commit carries the two-configuration harness — `ebf4289` and
`22a2512` benchmark the search with no deadline at all, so their own harnesses
never exercise the clock read. The `946091b` harness was therefore copied onto
detached worktrees of both so that all three rows measure the same two
configurations. Reproducing this table requires that copy; running
`cargo bench --bench search` at either earlier commit yields a single figure that
belongs in neither column.

Two things this table establishes:

1. **TASK-41 is the only change here that moved search speed.** It cut the
   deadline-bearing search by 18.8% (49.59 µs to 40.25 µs), a 23.2% NPS
   improvement, by sampling the clock every 8 nodes instead of on every
   `stopping()` call. Unthrottled deadline checking cost about 16–18 ns per
   node; the throttle removes roughly 95% of it.
2. **The TASK-45/46 abort-semantics rework did not change search speed.** The
   no-deadline column moves by about 1 µs across all three commits, which is
   inside the drift band. Any apparent improvement at that scale is noise.

An earlier TASK-41 measurement recorded a 70.467 µs baseline and a 41.2%
improvement. That baseline is **not reproducible**: the same commit under
controlled conditions measures 49.45 µs. The 70 µs figure was taken with
different Criterion settings on a machine that was evidently not idle, and it
inflated the apparent gain. The 18.8% figure above supersedes it. This is the
reason the search benchmark is documented here with an explicit methodology:
comparing numbers across sessions without controlling conditions produced a
confident claim that was wrong by more than a factor of two.

Run the search benchmarks with:

```sh
cargo bench --bench search
```

## Transposition table

`benches/tt.rs` measures the table directly, because the search benchmark above
cannot: its depth-7 tree is 579 nodes, which barely touches the hash. Measured at
TASK-57 (`849cdf5`) on the same Apple M3 Pro, `rustc 1.97.1`.

| Benchmark | Result |
| --- | ---: |
| `tt lifecycle/construct 256MB` | 19.26 ms |
| `tt lifecycle/clear 256MB` | 2.39 ms |
| `tt probe hit` | 36.46 ns |
| `tt probe miss` | 32.78 ns |
| `tt store` | 42.22 ns |
| `tt multi worker/1 workers, mixed probe/store` | 23.80 ms |
| `tt multi worker/4 workers, mixed probe/store` | 8.33 ms |

Run them with:

```sh
cargo bench --bench tt
```

### Retained lifecycle costs

Both lifecycle figures are costs the design accepts rather than costs it avoids,
so they are recorded rather than assumed negligible:

- **Construction is 19.3 ms for 256MB**, paid at `setoption name Hash`. It is
  zero-initialisation of the whole allocation, linear in size.
- **Clearing is 2.39 ms for 256MB**, paid at `ucinewgame`. The previous table
  cleared in constant time by advancing a generation counter, so this is a real
  regression at that boundary — deliberately taken. A generation bump leaves
  stale entries physically present, which forces the wrap case to walk the table
  anyway, and lets an entry come back to life if the counter ever laps. A linear
  clear of an allocation that has just been declared worthless, at 2.4 ms per
  256MB and once per game, buys exact invalidation.

The probe and store figures are on a 64MB table, far larger than cache, so each
includes the cache miss a real search pays. That miss dominates: all four slots
of a cluster share one 64-byte line, so scanning four candidates instead of one
costs arithmetic on data already in flight, not a second fetch.

The multi-worker figures run identical total work (1,000,000 mixed operations)
across 1 and 4 threads over one shared table with no key partitioning. Four
workers complete it 2.86× faster than one. The shortfall against 4× is memory
bandwidth, not table contention: the operations are unsynchronised relaxed loads
and stores with no compare-exchange, and workers contend for individual cache
lines only when they collide on the same cluster. What this benchmark is for is
catching the opposite result — throughput that fails to improve, or degrades,
with worker count would mean false sharing or replacement contention.

### Effect on search

Measured against the task's base commit `9b7bf33`, round-robin across two
worktrees over nine rounds, `go depth 10` from the start position at the default
16MB hash:

| Measure | Base `9b7bf33` | TASK-57 | Change |
| --- | ---: | ---: | ---: |
| Nodes to depth 10 | 4,883,269 | 4,762,311 | **2.5% fewer** |
| Best time to depth 10 | 882 ms | 891 ms | 1.0% slower |
| Best NPS | 5.54 million | 5.34 million | 3.4% lower |

Both engines return the same score and the same principal variation at every
depth.

The node count is exact and reproduces identically on every run, so the 2.5%
reduction is a real search-efficiency gain from four-way associative clusters and
depth- and age-aware replacement. The timings are not comparable at that
resolution: individual runs ranged from 882 ms to 1510 ms on the same binary, so
only the minimum of nine rounds is quoted, and a 1% difference between minima is
inside the drift. **Read this row as level, not as a regression and not as a
win.**

The NPS figures are the honest cost side. Roughly 3% of per-node throughput goes
to the new layout: a probe scans up to four slots rather than one, and a 16-byte
entry holding the full key gives half as many entries per megabyte as the old
8-byte entry did (visible as `hashfull` 607 against 294 at the same depth and
hash size). Fewer nodes and a slightly dearer node cancel out, which is the
trade this task made deliberately: full-key verification and snapshot-consistent
probes in exchange for entry density, at no net cost in time to depth.

The `cargo bench --bench search` harness was also run round-robin over three
rounds and showed the two commits level (base 42.48 µs, TASK-57 42.11 µs, best of
three, with deadline). That harness is not sensitive to this change and is
reported only to show it did not move.
