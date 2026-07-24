---
id: TASK-82
title: >-
  Diagnose the strength gulf vs Stockfish: NPS, effective branching factor, and
  eval quality
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-24 11:00'
updated_date: '2026-07-24 11:05'
labels:
  - search
  - eval
  - benchmark
  - investigation
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/eval.rs
  - BENCHMARKS.md
priority: high
type: spike
ordinal: 138000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
We are roughly 1000-1500 Elo behind the frontier and do not know how that gap decomposes. Playing strength factors into three axes; measure each against Stockfish on the same single-core hardware so effort can be allocated between a bigger NNUE and a faster/deeper search. Output is a report and a recommendation, not an engine change. Fully orthogonal - runs alongside other work. Using Stockfish only as a measurement reference is fine and does NOT touch the TASK-69 self-play-purity constraint (that governs training data, not benchmarking).

The three axes and how to measure each (note the corrected physics: selectivity buys depth-per-node, so it shows up on the fixed-time/fixed-node axis, NOT at fixed depth):

1. NPS (raw speed). Run 'go depth N' / bench on a fixed position suite, single thread, and record nodes/sec for seaborg and Stockfish. Our net is small, so our raw NPS may exceed SF's even though each SF node is worth more - which is exactly why NPS alone is not conclusive.

2. Selectivity / effective branching factor (depth-per-node). From the same runs, EBF ~= nodes^(1/depth); also record nodes-to-reach-depth-N and depth-reached-at-fixed-time (the latter combines NPS x EBF). Compare to SF. This is where reductions/pruning quality shows up.

3. Eval quality (per-position accuracy), isolated from search. Cleanest: a search-free static-eval agreement test - take a reference-labelled position set (label via deep Stockfish search or a public suite) and measure how well each engine's STATIC eval ranks the positions. Compare on a scale-independent metric (rank/Spearman correlation, or winner-prediction accuracy on decisive positions, or after mapping cp to a WDL logistic) - never raw centipawns, the scales differ. Optionally corroborate with a fixed-depth match with forward pruning/reductions/extensions disabled on both sides (approximate, since SF cannot be fully neutered).

Bounding the decomposition: also run short head-to-head gauntlets seaborg-vs-SF at fixed NODES and at fixed TIME. The fixed-nodes gap removes the NPS axis, so the residual is eval + selectivity-per-node; comparing to the fixed-time gap shows how much raw speed is worth.

Minimal tooling permitted within scope: a UCI helper to dump seaborg's static eval for a FEN if one does not already exist; small scripts to drive the runs. No hot-path change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 NPS for seaborg and Stockfish is measured on a fixed single-core suite and recorded, with the compile flags (AVX2), hardware, and SF version noted
- [ ] #2 Effective branching factor is reported for both engines (nodes^(1/depth) and/or nodes-to-fixed-depth), alongside depth-reached-at-fixed-time
- [ ] #3 Eval quality is isolated with a search-free static-eval agreement test against a reference-labelled position set, reported on a scale-independent metric, not raw centipawns
- [ ] #4 Head-to-head Elo gaps at fixed nodes and at fixed time are measured to bound how much of the gulf is speed versus eval-plus-selectivity
- [ ] #5 A written attribution decomposes the gulf into eval vs NPS vs selectivity and recommends where to invest (larger NNUE vs search speed/selectivity), with methodology recorded for reproducibility
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a search-free static-eval UCI dump to seaborg (the one permitted tooling change): an 'eval' command that prints the NNUE forward-pass score (and hand-crafted static_eval) for the current position, side-to-move perspective. No hot-path change.
2. Provision the measurement rig (AMD Ryzen 9 3900XT, AVX2, idle): build seaborg release with target-cpu=native from the task branch; use the vendored stockfish-ubuntu-x86-64-avx2 and ~/.local/bin/fastchess. Record SF version, flags, hardware. Measure only when the machine is idle.
3. AC#1 NPS: drive UCI 'go depth N' single-thread over a fixed FEN suite for seaborg and Stockfish; record nodes, nps.
4. AC#2 EBF: from the same runs compute nodes^(1/depth), nodes-to-fixed-depth, and depth-reached-at-fixed-time for both engines.
5. AC#3 Eval quality: label a position set with deep Stockfish search; dump seaborg static eval (NNUE) per FEN; compare on a scale-independent metric (Spearman rank / WDL-mapped accuracy), never raw cp.
6. AC#4 Head-to-head: fastchess seaborg-vs-Stockfish at fixed NODES and at fixed TIME; use an SF handicap ladder (reduced nodes/time) to find parity points so the gap is measurable, isolating NPS (fixed-time vs fixed-nodes).
7. AC#5: write the attribution (eval vs NPS vs selectivity) and the invest-recommendation into BENCHMARKS.md/report with full methodology for reproducibility. Small driver scripts committed under tools/. No engine hot-path change.
8. Run required checks (fmt/clippy/test), commit, hand off to review.
<!-- SECTION:PLAN:END -->
