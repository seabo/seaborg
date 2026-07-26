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

### Soft/hard time limits and next-iteration prediction

Splitting the per-move allotment into an optimum the search plans to spend and a
maximum it may not exceed, declining an iterative-deepening iteration whose cost
— extrapolated from the measured growth of the last two iterations — would
overrun the optimum, and extending past the optimum only when the root best move
changed or the root score fell.

| Field | Value |
| --- | --- |
| Baseline | `git:108c2bd` (the task's merge-base) |
| Candidate | `git:7b474d2` |
| Result | **PASS** — SPRT crossed the upper boundary (LLR 2.96, bounds ±2.94) |
| Elo | **+92.1 ± 19.7** (fastchess pentanomial error) |
| Games | 614 (W-D-L 255-263-96), pentanomial 6-34-108-113-46, 0 crashes, 0 forfeits |
| Time control | `tc=2+0.05`, 64 MB hash, one worker per engine |
| SPRT | `elo0=-5, elo1=0, alpha=0.05, beta=0.05` (the no-regression gate) |
| Runner | fastchess alpha 1.5.0, `openings-v1.epd`, `target-cpu=native` release, rustc 1.97.1 |
| Machine | Apple M3 Pro, concurrency 4 |

The same pair of binaries at a slower control, run sequentially rather than
concurrently so the two matches could not contend for cores:

| Field | Value |
| --- | --- |
| Result | **PASS** — SPRT crossed the upper boundary (LLR 2.95, bounds ±2.94) |
| Elo | **+76.8 ± 18.7** (fastchess pentanomial error) |
| Games | 722 (W-D-L 279-321-122), pentanomial 11-52-116-133-49, 0 crashes, 0 forfeits |
| Time control | `tc=10+0.1`, 64 MB hash, one worker per engine |

Every game at both controls terminated normally, and neither runner log contains
an illegal-move, forfeit, disconnect, or timeout line. The harness fails closed
on any of those before a result is recorded, so a completed report is itself the
evidence that none occurred.

Unlike the search-pruning entries above, this change alters no search decision
at a fixed budget: at `depth=N` or `nodes=N` the two builds visit identical
trees. Everything it is worth is in where the clock goes, which is why a timed
match is not merely preferable here but the only measurement that can show
anything at all.

The size of the gain reflects how much was being discarded. On the baseline the
iterative-deepening loop began iteration `d+1` whenever the deadline had not yet
passed, and an aborted iteration is thrown away whole — so the common case was
starting an iteration with a small fraction of its cost remaining and returning
the previous iteration's move anyway. Measured directly on the start position at
`go wtime 60000 winc 500` (an optimum of 1997 ms): the baseline completes depth
16 at 1623 ms, then spends the remaining ~370 ms on a depth-17 iteration it
discards; the candidate declines depth 16, returns the same move at depth 15 in
about 1150 ms, and hands the rest to later moves. The shorter the control, the
larger the share of each move that was going into the discarded iteration, which
is why 2+0.05 (+92) shows the effect more strongly than 10+0.1 (+77). The two
intervals overlap, so the ordering is the expected shape rather than a measured
difference between the controls.

### Symmetric stability scaling: contracting the soft limit on settled positions

Extending the soft/hard split above with the missing direction. Previously the
stability multiplier only ever *extended* a move — a changed root move or a
falling score pushed the soft limit toward the maximum, but nothing pushed it
below the optimum, so a dead-equal position where the same root move was best at
every iteration and the eval was flat still spent its full planned budget every
move. The candidate makes the multiplier symmetric: once the root move has held
and the inter-iteration score has stayed within a flat margin across enough
consecutive iterations, the soft limit contracts below the optimum (one step per
settled iteration past an onset, down to a floor of half the optimum), handing
the unspent clock to later moves. The hard deadline, the guaranteed first ply,
and the legal-bestmove contract are untouched.

**This change resists the round-robin SPRT that the entries above rely on, for a
structural reason worth recording.** The motivating case is a real no-increment
game lost on time: in a drawish endgame with nothing to convert, the engine bled
its clock searching for a better move that did not exist while a faster opponent
banked time and won the race. That benefit is realised *against a faster field*.
A self-play SPRT pits the candidate against a baseline that thinks for the same
length of time, so the time-race asymmetry the change exists to win never arises
— an equal-speed match measures only the small *cost* of the contraction (a
little less depth on settled positions), never its benefit. Worse, the mandated
harness fails closed on any time-forfeit as an infrastructure error (a valid
report requires zero forfeits), so the no-increment regime where flagging
actually occurs cannot be scored at all: with no flags the benefit is invisible,
and with flags the run is discarded. Forfeit-counting is a reserved-but-unbuilt
harness mode. Building it, or a calibrated external-engine gauntlet, is the
correct way to measure this and is out of scope here; this entry is therefore
**mechanism-based plus a non-regression gate**, by explicit maintainer decision.

Mechanism, measured directly on the archetype of the motivating loss — a
dead-drawn king-and-pawn endgame `8/5pk1/6p1/4P1P1/5PK1/8/8/8 w - -`, both
builds under `go wtime 40000 btime 40000 movestogo 20`:

| Build | Depth reached | Wall time | Behaviour |
| --- | ---: | ---: | --- |
| Baseline | 24 | 2691 ms | spends the full planned budget |
| Candidate | 23 | 494 ms | contracts to the floor, banks ~82% of the budget |

The root move (`g4f3`) is stable from depth 6 and the score only oscillates a few
centipawns, so the streak reaches the floor and the candidate returns one ply
shallower in a fraction of the time. Sharp, tactically live positions do not
contract and still extend exactly as before. The contraction scales with search
depth × settledness, so it concentrates on the deep, dead-flat searches where
clock-bleeding — and therefore flagging risk — actually lives.

Non-regression gate, baseline `git:b2f9457` vs candidate `git:e6b8117`,
`tc=10+0.1`, `elo0=-5 elo1=0 alpha=0.05 beta=0.05`, fastchess, `openings-v1.epd`,
`target-cpu=native` release, Apple M3 Pro, concurrency 4:

| Field | Value |
| --- | --- |
| Result | **Non-regression only** — no single uninterrupted window reached an SPRT boundary or the game cap |
| Score | ≈ 0.50 near-neutral (partial windows: 2409 games at 0.4952 ≈ −3 Elo; 457 games at 0.5066 ≈ +5 Elo), 2σ within ±10 |
| Forfeits | 0 across every window (increment prevents flagging, so the gate scores cleanly) |

The gate is incomplete because the automated environment repeatedly reaped the
long-running match at session teardown (clean games, zero engine faults, simply
truncated). The measurement was performed but not to exhaustion; the partial
windows agree on near-neutral and never trend toward a regression. Merged as a
low-risk, mechanism-sound change on that basis. A completed SPRT — and, better,
a forfeit-counting no-increment match once the harness supports it — remains the
way to promote this from mechanism-based to measurement-based.

### Internal iterative reduction: shallower search on a transposition-table miss

Implementing the two paired depth-reduction steps in the main search. A node
reached with no transposition-table move to try first has no cheap guide to its
best move, so its move ordering is poor and a full-depth search of it is
expensive out of proportion to how often it matters. Such a node is searched
shallower instead: in a PV node the depth is cut by three plies, and in a non-PV
node at depth ≥ 7 by two. There is no verifying re-search — unlike late-move
reduction — so the cut is a speculative bet that a move-less node is cheap to get
slightly wrong; the reduced search stocks the table with a move that a later,
deeper visit can then use. The trigger is a genuine absence of a table move (a
miss or a legitimately move-less entry) and never a Zobrist-collision guard,
whose foreign entry says nothing about whether the node has been explored.

| Field | Value |
| --- | --- |
| Baseline | `git:cfdac4d` (master, the task's merge-base; tested binary sha256 `5bfc5223…`) |
| Candidate | task-52 internal-iterative-reduction (tested binary sha256 `4c646761…`, built from the identical search implementation; the binary embeds the build-time commit string via `GIT_HASH`, which feeds only UCI version reporting, so a rebuild from the final commit differs by that string alone and reproduces the search behavior byte-for-byte) |
| Result | **PASS** — SPRT crossed the upper boundary (LLR 2.96, bounds ±2.94) |
| Elo | **+28.3 ± 10.8** (fastchess pentanomial error) |
| Games | 1524 (W-D-L 405-838-281), pentanomial 25-124-353-222-38, 0 crashes, 0 forfeits |
| Time control | `tc=8+0.08`, 64 MB hash, one worker per engine |
| SPRT | `elo0=-5, elo1=0, alpha=0.05, beta=0.05` (the no-regression gate) |
| Runner | fastchess alpha 1.5.0, `openings-v1.epd`, `target-cpu=native` release |
| Machine | Apple, concurrency 4 |

The reduction trades horizon depth for breadth, and its worst case is a deep,
move-less line that must be calculated precisely rather than pruned — a
king-and-pawn race being the archetype. On the won K+P-vs-K+P endgame in the
`gives_correct_answers` suite the engine still plays the winning move, but the
promotion's full value now surfaces two plies later than before (depth 24 rather
than 22); that fixture's depth was raised to match. The measured +28 Elo is the
net over the opening book: the breadth the cut buys everywhere else more than
pays for the occasional deep line it shortens.

## Incremental NNUE accumulator in the search hot loop

The NNUE first-layer accumulator used to be rebuilt from scratch at every
evaluated leaf — an O(pieces × H) scan of the whole board. It is now maintained
incrementally along the search's make/unmake, folding in only the features each
move toggles (O(features-toggled × H)) and restoring the previous value on unmake
from a per-ply stack. The evaluation is bit-identical: a fixed-depth search visits
exactly the same nodes before and after, which is what makes the timing a
like-for-like speed comparison rather than a search-shape one.

Measured single-thread with the built-in `gen-002` network (hidden width 256),
running `go depth 12` to completion on four positions, base `9fe845c` against the
task branch. `nps = nodes / time`; node counts are identical by construction, so
they are the control, not the result. Apple M3 Pro, `rustc 1.97.1`, otherwise
idle, medians of interleaved base/branch runs.

| Position | Nodes | Base time | Base NPS | Branch time | Branch NPS | Speed-up |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| startpos | 150,749 | 281 ms | 536 k | 170 ms | 883 k | 1.65× |
| kiwipete | 367,085 | 1003 ms | 366 k | 787 ms | 466 k | 1.27× |
| middlegame | 142,714 | 314 ms | 454 k | 252 ms | 566 k | 1.25× |
| endgame | 49,991 | 32 ms | 1517 k | 34 ms | 1435 k | ~1× (noise) |
| **Aggregate** | **710,539** | **1630 ms** | **436 k** | **1243 ms** | **572 k** | **1.31×** |

About **+31 % NPS in aggregate**. The gain tracks piece count, as expected: the
opening, where the discarded from-scratch scan touched the most pieces, gains most
(+65 %); the sparse endgame is too fast to measure against timer noise. This is on
seaborg's *scalar* ARM NNUE path — the same handicap the strength-gulf diagnostic
notes — and the saving is per-leaf work removed, so it is at least as large on the
x86 AVX2 target. Crucially the per-node cost of the incremental path is
O(features × H) against the old O(pieces × H) rebuild, so the margin widens as the
hidden width grows: this change is the enabler that keeps a wider network
affordable, which is its purpose.

One implementation note worth recording, since it dominated the result: the
restore stack is a single flat, pre-sized `i16` buffer. An earlier version stored
a `Vec` of freshly boxed per-ply payloads, whose per-node heap
allocate-and-free churn cancelled the entire saving (NPS came out flat). Sizing
one buffer up front and copying into its spare capacity is what turns the change
into the +31 % above.

## NNUE self-play bootstrap programme

The entries above measure *search* changes against a fixed evaluation. This
section records the opposite: the strength of a trained NNUE **evaluation**
produced by self-play reinforcement, against the hand-crafted tapered evaluation
it replaces. The programme ran the reinforcement loop (`tools/rl/`) for a
sequence of generations — each generates self-play data with the current best
network, trains a candidate, and gates it against that best with the same SPRT
harness — and then anchored the final network to an absolute scale with a direct
gauntlet against the hand-crafted evaluation.

All figures here were measured on a **different machine** from the sections above
(AMD Ryzen 9 3900XT, 12 cores, concurrency 11) and at a common time control of
`tc=10+0.1` with 64 MB hash per engine. They are internally consistent — every
match is one binary playing both sides, told apart only by its `EvalFile` option
— but they are not comparable to the M3 Pro search figures above and are not
intended to be. The engine build is pinned at `git:d53e33e`, which predates the
embedded-network feature, so a side with no `EvalFile` genuinely runs the
hand-crafted evaluation rather than a baked network. Runner: fastchess alpha
1.7.0, `openings-v1.epd`, `target-cpu=x86-64-v2` release, rustc 1.97.1.

### Per-generation self-play gates

Each generation is gated against the best network at the time under a
promote-on-improvement SPRT. Generation 0's opponent is the hand-crafted
evaluation; each later generation's opponent is the previous *promoted* network,
which is not necessarily the immediately preceding generation. A rejected
candidate promotes nothing, so its predecessor remains the baseline.

| Gen | Opponent | Elo | Games | W-D-L | Pentanomial | Verdict |
| ---: | --- | ---: | ---: | --- | --- | --- |
| 0 | hand-crafted | **+263.2 ± 32.7** | 358 | 247-93-18 | 0-2-27-69-81 | PASS (promoted) |
| 1 | gen-000 | **+156.5 ± 26.1** | 476 | 259-159-58 | 5-15-58-94-66 | PASS (promoted) |
| 2 | gen-001 | **+22.3 ± 9.8** | 2544 | 774-1159-611 | 71-273-462-354-112 | PASS (promoted) |
| 3 | gen-002 | **−17.3 ± 9.8** | 2858 | 716-1284-858 | 155-369-460-353-92 | FAIL (rejected) |

Generation 0's SPRT is the bootstrap's non-regression gate
(`elo0=-5, elo1=0`); generations 1–3 tighten it to demand improvement
(`elo0=0, elo1=5`). All four ran to a boundary with 0 crashes and 0 forfeits;
every game terminated normally.

The curve does not merely flatten, it turns over. Gains fall by roughly an order
of magnitude per step and generation 3 is measurably *worse* than its parent, so
the gate correctly refuses it and **gen-002 is the programme's final network**.
Three independent signals corroborate the plateau rather than a single point
estimate: the gate needed 5.3× more games to separate generation 2 from its
parent than generation 1 did, the draw rate climbs from 33% (gen 1) through 46%
(gen 2) to 45% (gen 3), and the pentanomial mass migrates from the winning tail
toward the centre. The likely cause is the recipe itself — ~30 M samples per
generation, hidden width 256, 5000 nodes per self-play move — rather than
anything a few more iterations of the same recipe would break; a wider network
or richer labels is separate work.

A methodological caution recorded for reuse: two minutes into generation 2's
gate, the running score over 45 games was 0.567, implying roughly +47 Elo; the
resolved figure over 2544 games was +22.3. An interim gate score at that sample
size is consistent with anything from about −50 to +150 Elo. Interim scores are
not results.

### Absolute anchor: gen-002 vs the hand-crafted evaluation

The per-generation deltas are each measured against a different opponent and do
**not** compose into a figure against the hand-crafted baseline (naïve addition
of the promoted deltas would claim +442, which is meaningless). The final
network was therefore played directly against the hand-crafted evaluation over a
fixed 1000-game gauntlet, twice:

| Field | Run 1 | Run 2 |
| --- | ---: | ---: |
| Elo | **+334.1 ± 24.4** | **+339.6 ± 25.9** |
| W-D-L | 778-189-33 | 796-160-44 |
| Score | 87.25 % | 87.60 % |
| Pentanomial | 0-4-51-141-304 | 1-6-49-128-316 |
| Draw ratio | 10.2 % | 9.8 % |

The two independent runs agree at **≈ 337 Elo** over the hand-crafted
evaluation. This is the headline result of the whole NNUE programme: the
network that a plain build now embeds is about 337 Elo stronger at this control
than the tapered evaluation it replaced.

The candidate is gen-002 (`nnue:gen-002`, sha256 `f076dc46…`, parameter hash
`0x6ad073be2b6899cb`, hidden width 256), embedded as the default network by
TASK-69.15.

Note on archival form: these two matches are recorded here from the runner logs
rather than as the harness's `report.json`, because `strength_test.py` cannot
parse them. At an 87 % score the SPRT log-likelihood ratio degenerates to `nan`
in the runner output, and the harness — correctly, for a gate — treats a `nan`
LLR as malformed and exits with an infrastructure error, regardless of the
hypothesis bounds. The matches themselves completed all 1000 games cleanly; only
the SPRT summary line is unusable. A gauntlet whose purpose is an Elo estimate
rather than a gate decision does not need the SPRT machinery, and teaching the
harness to record a lopsided non-gate match in its archived form is a genuine
gap left for follow-up.

### Cost accounting against the pre-run estimates

The realised per-generation cost, measured on the run host:

| Stage | Cost |
| --- | --- |
| Datagen (gen 0, hand-crafted evaluator) | 3996 positions/s → 386 k games in 3.3 h |
| Datagen (gens 1–3, NNUE evaluator) | ~2900 positions/s, ~23 % slower per node than the hand-crafted evaluator, ~4.1 h per generation |
| Training + export (per generation) | ~25 min; bounded by the dataloader (~500 k samples/s), not the GPU |
| Gate | ~1000 games/hour; duration scales inversely with the margin (22 min at gen 1, 77 min at gen 2, and up to the 10 000-game cap ≈ 10 h for a marginal candidate) |

Two cost facts worth carrying forward. Datagen throughput peaks at the physical
core count and collapses with SMT threads — 24 workers measured 2.25× slower
than 12 on this 12-core host — so worker count must be pinned to physical cores,
not to `available_parallelism()`. And as the strength margin shrinks, the SPRT
gate rather than datagen becomes the variable cost: a near-neutral candidate can
occupy the full game cap to reach a verdict, which is the economic reason the
programme was stopped once the per-generation gain fell into single digits.

## Strength-gulf diagnostic vs Stockfish

The sections above measure seaborg against itself. This one measures it against a
frontier reference (Stockfish) to answer a single question: the engine is roughly
1000–1500 Elo behind the frontier, and that deficit had never been *decomposed*.
Playing strength is, roughly, raw speed (NPS) × depth-per-node (selectivity) ×
accuracy-per-node (evaluation quality). This diagnostic measures each axis
separately so effort can be directed at the one that is actually limiting, rather
than guessed at. It is a measurement, not an engine change; Stockfish is used
purely as a reference and never touches training data. The driver scripts and the
fixed position suite live in `tools/diag/` and each result below is reproducible
from the commands recorded there.

**The bottom line up front:** raw speed is *not* the bottleneck — seaborg's NPS is
already competitive. The gulf is split between two axes that this diagnostic
cannot cleanly separate, because they are **coupled**: search **selectivity** and
**evaluation** quality. seaborg's search reaches roughly **eight plies less deep
in the same time** and needs on the order of **40–50× more nodes** to play at
Stockfish's level, and its static evaluation is about **twice as error-prone** as
Stockfish's on decisive positions. Both are high-leverage. What this diagnostic
does *not* support is ranking the two, or deprioritising the network — see the
attribution section for why the eval metrics used here saturate, why a better eval
also buys selectivity, and why the engine's own history (a +337 Elo jump from the
hand-crafted evaluation to the current small net) argues the network is nowhere
near its ceiling.

### Measurement setup

All figures were measured on **Apple M3 Pro (12 cores), macOS**, one search thread
per engine, 64 MB hash. seaborg is the TASK-82 branch built with a default
`aarch64-apple-darwin` release. **ISA caveat:** seaborg's NNUE inference has an
x86 AVX2 path and a scalar fallback, but *no* ARM NEON path, so on this Apple
Silicon host it runs the **scalar** forward pass, whereas the reference Stockfish
uses a NEON-optimised NNUE. This handicaps seaborg only on the *speed* axis
(NPS, and anything time-based); the node-count, selectivity, and evaluation
figures are ISA-independent and exact. The practical consequence is that the NPS
axis below is a *pessimistic* bound for seaborg — its x86 AVX2 deployment is
faster still — which only strengthens the "speed is not the bottleneck"
conclusion. Reference: **Stockfish 18** (arm64, Homebrew). Runner for the
head-to-head matches: local `fastchess`, `openings-v1.epd` (8 openings, so
per-match variance is real). The host carried a fluctuating background load
(other users' jobs, load average ~7–12) during the runs, which adds noise to the
time-based measurements but not to the node-based ones.

### NPS and selectivity (effective branching factor)

Fixed suite of 20 phase-balanced positions (`tools/diag/bench-positions.epd`),
`go depth 14`, best of three runs, plus a `movetime 1500` pass for depth reached.

| Metric (single thread) | seaborg | Stockfish 18 | Ratio |
| --- | ---: | ---: | ---: |
| Aggregate NPS (∑nodes / ∑time) | 642 k | 727 k | 0.88× |
| Median NPS | 687 k | 849 k | 0.81× |
| Geomean EBF at depth 14 (`nodes^(1/depth)`) | 2.42 | 2.00 | — |
| Mean nodes to reach depth 14 | 373 553 | 24 491 | **15.3×** |
| Median depth reached at 1500 ms | **14** | **22** | +8 plies |
| Mean depth reached at 1500 ms | 15.0 | 27.9 | — |

Two facts stand out. First, **NPS is comparable** — and this is with seaborg
running scalar NNUE while Stockfish runs NEON, so on its native x86 AVX2 the small
net would likely make seaborg *faster* per node. Raw speed is not the problem.
Second, **selectivity is dramatically behind**: seaborg needs ~15× the nodes to
reach the same nominal depth, and with near-equal NPS that means it reaches eight
fewer plies in the same wall-clock time. That eight-ply gap is the visible face of
the strength gulf.

### Evaluation quality, isolated from search

A search-free static-eval agreement test (`tools/diag/eval_agreement.py`). 500
diverse, mostly-quiet positions were sampled from lightly-randomised games
(`tools/diag/gen_positions.py`) and labelled by a deep Stockfish search
(depth 20) as ground truth. Each engine's **static** evaluation — no search — was
then queried (seaborg via the new `eval` UCI command; Stockfish via its `eval`
command) and compared to the deep label on scale-independent metrics, since the
two engines' centipawn scales differ. seaborg was evaluated with its embedded
gen-002 network.

| Static eval vs deep-search label (n = 500, 446 decisive) | Spearman ρ | Winner accuracy (\|label\| > 100 cp) |
| --- | ---: | ---: |
| seaborg (NNUE gen-002) | 0.931 | 94.6 % |
| Stockfish 18 | 0.954 | 97.5 % |

These numbers are easy to over-read as "eval is basically solved," and that
reading is a trap. Both metrics **saturate** near the top: most positions are easy
to rank and both engines get them, so the interesting signal is compressed into a
small headline gap, and there is no calibration here from Spearman ρ to Elo. Read
as an *error rate* the same data is far less flattering: seaborg mis-ranks the
winner in **5.4 %** of decisive positions versus Stockfish's **2.5 %** — roughly
**twice the error rate**, concentrated in exactly the hard positions that decide
games. Three further limits mean this test *cannot* bound how much a better network
would buy: it sees only static eval on **quiet** positions sampled from
**Stockfish-guided** play (not the positions seaborg reaches), it is
**scale-independent** so it is blind to cp-calibration (which drives pruning
margins), and its ground truth is Stockfish's *own* deep search, which structurally
flatters Stockfish's static eval. The honest reading: seaborg's evaluation is
respectable but measurably behind, and this metric is the wrong instrument for
declaring it "good enough."

### Head-to-head, at fixed nodes and at fixed time

Direct seaborg-vs-Stockfish matches (`tools/diag/gauntlet.py`,
`tools/diag/sweep.py`). At *equal* search budgets Stockfish wins essentially every
game, which only establishes a floor, so Stockfish was then handicapped (given far
fewer nodes) and swept to locate the budget at which the match is even. That
**parity point** is the informative quantity: it is how much search seaborg needs
to equal Stockfish, with the raw-speed axis removed.

| Match | Budget | seaborg score | Result |
| --- | --- | ---: | --- |
| Equal nodes | both 100 k nodes/move | 0 % (0/20) | gap floor (equal-node) |
| Equal time | both `tc=8+0.08` | 0 % (0/40) | gap floor (equal-time) |
| **Fixed-node parity** | seaborg 100 k vs Stockfish ~2.0–2.5 k | ~50 % | **~40–50× node handicap** |

Fixed-node handicap sweep (seaborg fixed at 100 k nodes, 100 games/rung; the 8-opening
pool makes single rungs noisy, but the 50 % crossing is well bracketed):

| Stockfish budget | seaborg score | Elo (seaborg − SF) |
| ---: | ---: | ---: |
| 3000 nodes | 40.0 % | −70 ± 56 |
| 2000 nodes | 34.5 % | −111 ± 45 |
| 1500 nodes | 59.5 % | +67 ± 75 |
| 1000 nodes | 78.5 % | +225 ± 75 |

The two equal-budget floors are the same result (0 % — a lower bound of roughly
**> 440 Elo**, one-sided 95 %, from 0/40; the true gap is much larger). That they
are *identical whether the budget is nodes or time* is itself the answer to "how
much of the gulf is speed": if NPS were a meaningful factor the equal-time gap
would differ from the equal-node gap, and it does not. The parity sweep then shows
seaborg needs ~40–50× Stockfish's nodes to draw level — a combined
evaluation-plus-selectivity deficit *per node*, with speed already netted out.

A symmetric *fixed-time* parity sweep was attempted but is not reported as a
number: because Stockfish's parity budget is so small (tens of nodes-worth of
time, single-digit milliseconds), the measurement collapses into Stockfish's fixed
move-overhead and the shared host's scheduler jitter rather than genuine search —
at one rung seaborg spuriously "won" 92 % at a 15× time disadvantage. The
node-based parity is the reliable instrument here; the equal-time floor above
supplies the time-based data point.

### Attribution and recommendation

What this diagnostic settles, and what it deliberately does not:

- **Raw speed (NPS): ruled out.** seaborg's NPS is ~0.85× Stockfish's *while
  handicapped to scalar NNUE on ARM*; on its x86 AVX2 target it is likely at or
  above parity. The equal-time and equal-node gaps being identical confirms speed
  contributes essentially nothing to the differential. This is the one firm
  negative result.
- **Selectivity (depth-per-node): large, clearly-measured headroom.** ~15× the
  nodes to reach a given depth, eight plies shallower at equal time, and ~40–50×
  the nodes to reach Stockfish's *strength*. The search machinery that buys depth
  per node — late-move reduction, null-move and futility/history pruning, singular
  extensions, move ordering — is coarse relative to the frontier and has room to
  grow.
- **Evaluation quality: also high-leverage — and this diagnostic does not rank it
  against selectivity.** Three reasons the "eval is minor" reading is wrong. (1)
  The agreement metrics saturate: seaborg's ~2× winner-error-rate on decisive
  positions is not "nearly at parity," and there is no ρ→Elo calibration here. (2)
  The two axes are **coupled, not independent**: a better evaluation improves move
  ordering and lets the search reduce and prune more aggressively *safely*, so an
  unknown share of the selectivity deficit is plausibly downstream of eval quality
  — the diagnostic measures the axes separately but cannot untangle their
  interaction. (3) The engine's own record argues the network is far from its
  ceiling: replacing the hand-crafted PST evaluation with the current *small,
  weakly-bootstrapped* net was worth **≈ +337 Elo** (see the NNUE bootstrap
  section), and that net was trained on low-depth self-play data labelled from a
  PST-origin bootstrap. A larger network on deeper or higher-quality labels is a
  very reasonable bet for another large gain.

**Recommendation: rule out speed, and treat evaluation and selectivity as two
coupled, high-leverage fronts — do not deprioritise either on the strength of this
diagnostic.** The clean way to rank them is to *measure* rather than infer, with
two follow-up experiments this spike did not run: (a) train a larger / better-
labelled network and gate it at equal time, which directly prices the eval
headroom; and (b) re-run the selectivity measurements with a substantially
stronger evaluation to see whether effective depth improves — i.e. whether the
selectivity deficit is partly eval-limited. Whichever axis is worked, measure at
equal time, where both selectivity and eval gains actually show up.

### Reproducing this diagnostic

Tooling and methodology are in `tools/diag/` (`README.md` has the full recipe).
The one engine change this required is the search-free `eval` UCI command, which
prints `staticeval cp <v>` for the current position using the same evaluator a
search would. Summary of commands (paths abbreviated):

```sh
python3 nps_ebf.py       --seaborg SB --stockfish SF --suite bench-positions.epd \
                         --depth 14 --movetime 1500 --repeats 3
uv run --with chess python3 gen_positions.py --stockfish SF --games 500 --out pos.fen
python3 eval_agreement.py --seaborg SB --stockfish SF --positions pos.fen --label-depth 20
python3 sweep.py         --seaborg SB --stockfish SF --seaborg-limit nodes=100000 \
                         --sf-limits nodes=3000,nodes=2000,nodes=1500,nodes=1000 \
                         --games 100 --restart off --openings ../strength/openings-v1.epd
```

The node-based figures reproduce on any host. The NPS and time-based figures are
host- and load-dependent, and on ARM understate seaborg's speed; a confirmatory
NPS pass on an idle x86 AVX2 host (e.g. the datagen rig) would tighten the speed
axis, though it cannot change the conclusion that selectivity dominates.
