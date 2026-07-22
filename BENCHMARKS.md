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

## Transposition-table hot-path enhancements

Two hot-path candidates were evaluated against the hash-loading search benchmark
rather than adopted on the usual folklore: storing a position's static evaluation
in its entry, and prefetching a child's cluster before the recursive descent.
Storing the eval was rejected on the arithmetic below; prefetching was retained.
Both records exist so neither experiment is rediscovered from scratch.

### The measurement harness

`cargo bench --bench search` gained a `search hash load` group. The pre-existing
`search startpos depth 7` pair cannot see a transposition-table change: criterion
re-runs its closure against a table the previous iteration left warm, which
answers nearly every probe with an immediate cutoff and collapses the tree from
135k nodes to 579. The new group instead searches four positions to fixed depths
large enough to load a 16MB table, clearing the table *outside* the timed region
so every iteration searches the whole tree.

Node counts and probe outcomes are printed before the timings, because elapsed
time alone cannot attribute a change: a search that finishes sooner over the same
nodes got cheaper per node, one that finishes sooner over fewer nodes got better
informed, and the two call for opposite conclusions. Unlike the timings these
figures are exact and reproduce run to run.

| Position | Nodes | Probes | Hit rate | `hashfull` |
| --- | ---: | ---: | ---: | ---: |
| `startpos depth 9` | 2,501,994 | 2,501,994 | 45.6% | 648 |
| `kiwipete depth 8` | 5,241,036 | 5,241,036 | 20.6% | 1000 |
| `middlegame depth 8` | 5,780,828 | 5,780,828 | 21.3% | 1000 |
| `endgame depth 11` | 1,839,611 | 1,839,611 | 48.2% | 513 |

Per-node cost derived from these is about 75–82 ns: `startpos` runs 2.50M nodes
in a clean ~187 ms, `endgame` 1.84M in ~150 ms. That figure is the denominator
for both decisions below.

Run it with:

```sh
cargo bench --bench search -- "hash load"
```

### Rejected: storing the static evaluation in the entry

Two facts, either sufficient on its own, reject it.

**It does not pay.** A `static evaluation` benchmark group measures one
`material_eval` call at **2.8 ns** across all four positions. Against a ~78 ns
node that is 3.6%, and 3.6% is an unreachable ceiling, not the expected saving: a
value must be computed at least once to be stored, so the recompute is only ever
avoided on a *later* probe that hits, and only 20–48% of probes hit. The
realistic saving is a fraction of a fraction of one node's cost. This engine's
evaluation is ten popcounts on bitboards already in cache; the technique exists
for evaluations that are expensive, which this is not.

**It does not fit.** The data word documents exactly 15 spare bits (`bits 48..63`,
the `RESERVED_MASK`), and those bits are the entry's entire migration headroom —
what lets a future field be added without rewriting every stored entry. An `i16`
evaluation needs 16 bits, so it would not merely spend that headroom but overrun
it, forcing the entry from 16 bytes to a wider slot and halving entries per
megabyte a second time on top of the density already traded away at TASK-57.

**The imminent pruning consumers do not need it either.** TASK-50's futility and
null-move pruning read the static evaluation of the node they are *already at*,
which the search computes at step 6 before either pruning step is reached. Neither
wants an ancestor's or a stored eval, so a table-resident eval buys them nothing.

The condition to revisit is explicit: if the evaluation stops being material-only
— a piece-square table, or an NNUE whose per-call cost is tens to hundreds of
nanoseconds — re-run the `static evaluation` group and redo this arithmetic. At
that point the saving may justify a wider entry. It does not now.

### Retained: prefetching the child cluster

`Table::prefetch` issues a hardware prefetch hint (`_mm_prefetch` on x86_64, a
`prfm pldl1keep` hint on aarch64 since `core::arch::aarch64::_prefetch` is still
unstable, and an empty body elsewhere). The search calls it immediately after
`make_move`, in both the main search and quiescence, at the earliest point the
child's key exists — so the cache miss the child's probe would take begins
overlapping the recursive descent instead of stalling in front of it.

It is retained on mechanism and risk rather than on a measured speedup, because a
clean speedup could not be obtained: every benchmarking round of this task ran on
a machine carrying sustained load from other worktrees' benchmarks (load average
4–6 throughout), and a prefetch benchmark is precisely the worst case for that
contention, since its entire mechanism is hiding memory latency that a contended
memory bus changes. The minimum-of-six-rounds figures were `startpos` 197.3 →
185.6 ms (−5.9%) and `endgame` 154.3 → 155.6 ms (+0.8%): a non-negative direction,
clearly positive on the position with the coldest table, but not a repeatable
figure, and the base floors here sit above the ~187 ms a genuinely idle run
produced, so even the minima are contaminated. **Do not cite these percentages as
the effect; cite them only as the reason the effect could not be pinned down.**

What justifies keeping it without that number:

- **Zero search-quality risk.** A prefetch changes no architecturally visible
  state, so node counts are identical by construction — verified, not measured.
  There is no efficiency component to trade against, only per-node cost.
- **The hint is never wasted.** The prefetched cluster is exactly the one the
  child immediately probes, so it cannot pull in a line the search does not use.
- **The mechanism is standard.** Prefetching the transposition entry right after
  the move is made is textbook practice in strength-leading engines.

The cost side is one `unsafe` block per supported architecture. On x86_64
`_mm_prefetch` is unsafe only for taking a raw pointer and cannot fault; on
aarch64 the hint is hand-written inline assembly with no memory or flag effects.
`prefetch_moves_no_observable_state` pins the correctness contract — the hint
perturbs nothing a probe returns, for a stored key, a cluster sibling, and an
unstored key alike — and passes on a target whose prefetch compiles to nothing.

If this machine, or any documented idle machine, later yields a clean
round-robin, record the quantified figure here and promote the decision from
mechanism-based to measurement-based.

## Search strength results

Unlike the sections above — which measure per-node cost and fixed-depth node
counts — this section records **playing-strength** deltas from a round-robin
match at a real time control. A time control, not a fixed node or depth budget,
is mandatory for a search-pruning or reduction change: a node budget rewards a
more aggressive reduction with free extra depth it never pays for, inflating the
apparent gain. Only a clock charges for the re-searches an over-aggressive
reduction triggers, so only a timed match reports the true trade.

### Late-move reduction: log-based table with history and node-type modulation

Replacing the coarse two-step late-move reduction with a precomputed
`ln(depth) * ln(move_count)` reduction table, modulated by the move's own quiet
history (main plus continuation), the improving signal, and whether the node is
a PV node or the move is a killer/counter.

| Field | Value |
| --- | --- |
| Baseline | `git:708486f` (engine code identical to the task's merge-base `c4a6558`) |
| Candidate | `git:e8684e9` |
| Result | **PASS** — SPRT crossed the upper boundary (LLR 2.95, bounds ±2.94) |
| Elo | **+84.6 ± 20.1** (fastchess pentanomial error) |
| Games | 670 (W-D-L 280-270-120), pentanomial 9-46-114-108-58, 0 crashes, 0 forfeits |
| Time control | `tc=8+0.08`, 64 MB hash, one worker per engine |
| SPRT | `elo0=-5, elo1=0, alpha=0.05, beta=0.05` (the no-regression gate) |
| Runner | fastchess alpha 1.5.0, `openings-v1.epd`, `target-cpu=native` release, rustc 1.97.1 |
| Machine | Apple M3 Pro, concurrency 4 |

The large gain is expected rather than surprising: on the baseline the reduction
was nearly inert — the reduced scout searched at almost the raw depth — so this
is the first refinement to make late-move reduction actually widen the effective
search. The four refinements each sit behind a compile-time toggle
(`LMR_LOG_TABLE`, `LMR_HISTORY_MODULATION`, `LMR_IMPROVING_MODULATION`,
`LMR_FAVOURED_MODULATION`), so a future match can flip one off and rebuild to
attribute strength to it individually; this entry records the net effect of all
four against the pre-refinement baseline.
