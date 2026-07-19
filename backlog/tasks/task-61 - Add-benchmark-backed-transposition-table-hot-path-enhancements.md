---
id: TASK-61
title: Add benchmark-backed transposition-table hot-path enhancements
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 19:55'
labels:
  - transposition-table
  - performance
  - search
  - benchmark
dependencies:
  - TASK-60
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: medium
type: enhancement
ordinal: 60000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the identity policy, clean transposition-table rewrite, and search integration are stable, evaluate remaining hot-path opportunities rather than adopting them on folklore alone. The principal candidates are storing a position’s static evaluation to avoid duplicate work and support pruning, and prefetching child buckets before recursive search. Coordinate with TASK-50, TASK-51, and TASK-52 so metadata supports forthcoming pruning without coupling this task to those search changes. TASK-43 separately owns TT-assisted PV extension.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Representative fixed-depth positions and a reproducible benchmark establish baseline nodes, elapsed time, and probe behavior before hot-path changes
- [ ] #2 The value and validity conditions for a stored static evaluation are specified, including interaction with rule-sensitive evaluation from TASK-58; it is implemented only if measurements or imminent pruning consumers justify its entry-space cost
- [ ] #3 Child-bucket prefetching is evaluated on supported targets and retained only if it produces a repeatable benefit without harming portability or safety
- [ ] #4 Accepted enhancements include regression and benchmark coverage; rejected candidates have their measurements and decision recorded so the experiment is not repeatedly rediscovered
- [ ] #5 The final entry layout remains compact and its memory footprint and cache-line organization are asserted or tested
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a reproducible hash-loading search benchmark. The existing `search startpos depth 7` tree is 579 nodes and barely touches the table, so it cannot measure a TT hot-path change. Add a criterion group over representative fixed-depth positions whose trees are large enough to miss cache, and record baseline nodes, elapsed time and probe/hit/miss telemetry in BENCHMARKS.md.
2. Specify the value and validity conditions for a stored static evaluation before writing any code: what makes it reusable, how it interacts with the rule-sensitive evaluation policy, and what it costs in entry space given the data word has only 15 spare bits against the 16 an i16 eval needs.
3. Measure the static-eval candidate against the baseline (nodes, time, entry-space cost) and against the imminent pruning consumers in TASK-50/51/52. Implement only if the measurement or a concrete consumer justifies it; otherwise record the measurement and the decision.
4. Evaluate child-bucket prefetching: add a portable prefetch hint on the supported targets, issue it after make_move so the child cluster is in flight during the descent, and measure round-robin against the baseline. Retain only on a repeatable benefit with no portability or safety cost.
5. Add regression and benchmark coverage for whatever is accepted; write the measurements and rejection rationale for whatever is not into BENCHMARKS.md so the experiment is not rediscovered.
6. Assert the final entry layout: size, alignment, cluster-per-cache-line organisation and the reserved-bit invariant.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Evaluated two TT hot-path candidates against a new hash-loading benchmark; retained the prefetch, rejected storing the static eval. Full measurement narrative is in BENCHMARKS.md under 'Transposition-table hot-path enhancements'.

### Benchmark harness (AC#1)
- benches/search.rs gains a 'search hash load' group over four positions (startpos d9, kiwipete d8, middlegame d8, endgame d11) at fixed depths that load a 16MB table to 51-100% occupancy. Table is cleared outside the timed region so every iteration searches the whole tree; the pre-existing depth-7 pair cannot see a TT change because criterion re-runs it against a warm table (135k nodes collapse to 579).
- Exact, run-to-run-reproducible baseline printed before timings: startpos 2,501,994 nodes / 45.6% hit / hashfull 648; kiwipete 5,241,036 / 20.6% / 1000; middlegame 5,780,828 / 21.3% / 1000; endgame 1,839,611 / 48.2% / 513. Per-node cost ~75-82 ns.
- Added Search::trace() to expose the tracer for telemetry.

### Static eval, rejected (AC#2, AC#4)
- New 'static evaluation' microbench: material_eval is 2.8 ns, i.e. 3.6% of a ~78 ns node, and that is an unreachable ceiling (must compute once to store; only 20-48% of probes hit). Rejected on cost.
- Also rejected on entry space: the data word has exactly 15 spare reserved bits (the entry's only migration headroom); an i16 eval needs 16, so it would widen the 16-byte slot and halve density again on top of TASK-57.
- TASK-50/51/52 interaction: futility and null-move pruning read the eval of the node they are already at (search step 6), not an ancestor's or a stored one, so a table-resident eval buys the imminent pruning consumers nothing.
- Revisit condition recorded: a non-material-only evaluation (PSQT/NNUE, tens-hundreds of ns) changes the arithmetic.
- Interaction with TASK-58 rule-sensitive policy: not applicable, because the candidate was not implemented. A stored eval would have been position-intrinsic (evaluate() does not read the clock), consistent with TASK-58 rule 3, but no such field exists.

### Prefetch, retained (AC#3)
- Table::prefetch: _mm_prefetch (x86_64), inline 'prfm pldl1keep' (aarch64, since core::arch::aarch64::_prefetch is unstable), empty body elsewhere. Called after make_move in both main search and quiescence, at the earliest point the child key exists.
- Retained on mechanism/risk, not a measured figure: node counts identical by construction (a hint changes no visible state), the prefetched cluster is exactly what the child probes, and the mechanism is standard. A clean speedup was unobtainable: every round ran under sustained concurrent load (load avg 4-6 from other worktrees' benchmarks), which is the worst case for a latency-hiding benchmark. Minimum-of-6 was startpos -5.9%, endgame +0.8% (non-negative, not repeatable); documented as inconclusive, not cited as the effect.
- Decision to keep on mechanism grounds was confirmed with the user this session.
- Cost: one unsafe hint per architecture. prefetch_moves_no_observable_state pins that the hint perturbs nothing a probe returns and is total over keys.

### Entry layout (AC#5)
- Layout is unchanged (only a method was added), so the existing cluster_is_one_cache_line_and_slots_fill_it test still asserts the final layout: Cluster 64 bytes / align 64, Slot 16 bytes, 4 slots per cache line. clusters_are_cache_line_aligned_in_the_allocation covers alignment in the allocation.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 19:55
---
Implementation handoff
Branch: task-61-tt-hot-path-enhancements
Worktree: /Users/seabo/seaborg-worktrees/task-61-tt-hot-path-enhancements
Base: c55508b3383577ed9bb62a9ebadb21fc3ecedc1f
Implementation target: b76a0c234169623d7e5d519b1f34bc7c052fb74c
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (43 + 245 + 17 + 1 passed; 2 ignored are the pre-existing long perft suites)
- cargo bench --bench search -- "hash load": runs; baseline table reproduces
Known failures: none

Reviewer note: AC#3's repeatable-benefit measurement could not be obtained on this machine; it carried sustained load (avg 4-6) from concurrent worktree benchmarks for the whole session, and a prefetch benchmark is the worst case for that. The prefetch is retained on mechanism and risk (node-count-neutral by construction, hint never wasted, standard technique, contained unsafe cost), a call confirmed with the user this session. The inconclusive figures and full rationale are in BENCHMARKS.md. If a genuinely idle machine is available, a clean round-robin of 'search hash load' base vs target would let the decision be promoted from mechanism-based to measurement-based.
---
<!-- COMMENTS:END -->
