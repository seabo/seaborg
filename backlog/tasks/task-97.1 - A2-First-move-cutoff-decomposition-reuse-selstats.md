---
id: TASK-97.1
title: 'A2: First-move-cutoff decomposition (reuse selstats)'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-29 18:45'
updated_date: '2026-07-29 18:49'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - tools/diag/selectivity_profile.py
  - engine/src/ordering.rs
parent_task_id: TASK-97
priority: high
type: spike
ordinal: 166000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track A / item A2. Hypothesis: most of the ordering loss is concentrated at no-TT-move nodes. Reuse tools/diag/selectivity_profile.py at fixed depth and fixed nodes — it already emits first-move-cutoff split by TT hit/miss, the cutoff move-index histogram, and TT availability, so no new code is needed. Decision value: confirms WHERE to aim (quantifies the 92.7% with-TT vs 86.4% no-TT cutoff gap against the ~67% of cutoffs that occur at no-TT nodes) and scopes A1. Run this first — it is the cheapest item and scopes the rest of Track A.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The selectivity profile is re-run at fixed depth and fixed nodes on bench-positions.epd, reporting first-move-cutoff split by TT-hit vs TT-miss, the cutoff move-index histogram, and TT-move-availability by node type
- [ ] #2 The share of total beta cutoffs occurring at no-TT-move nodes and the TT vs no-TT first-move-cutoff gap are quantified against the recorded baseline
- [ ] #3 No permanent engine change ships; only diagnostic output is produced
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Build the behaviour-transparent instrumented binary: RUSTFLAGS="-C target-cpu=native" cargo build --release --features selstats. A selstats build searches the identical tree as default, so the profile is unbiased.
2. Run tools/diag/selectivity_profile.py at both operating points on the fixed suite: --seaborg target/release/seaborg --suite tools/diag/bench-positions.epd --depth 14 --nodes 2000000 --hash 64 --out (gitignored *.json; no permanent artifact ships, satisfying no-engine-change).
3. AC#1 — read the three signals from each aggregate: first_move_cutoff_rate_tt vs _nott, cutoff_index_dist (move-index histogram), and tt_move_avail / _pv / _nonpv.
4. AC#2 — quantify the share of total beta cutoffs at no-TT nodes (cutoff_share_nott) and the TT vs no-TT first-move-cutoff gap; compare both to the recorded baseline (fixed-depth-14: 88.5% overall, 92.7% TT vs 86.4% no-TT, 67% of cutoffs at no-TT nodes; fixed-2M: 89.4%, avail 35.4%). Note any drift.
5. Record the decomposition and the A2 conclusion (is ordering loss concentrated at no-TT-move nodes? -> scopes A1 oracle ceiling) in implementation notes; refresh BENCHMARKS.md selectivity figures only if stable. No source changes; run fmt/clippy/test as the clean gate.
6. AC#3 — verify no permanent engine change ships; only diagnostic output.
<!-- SECTION:PLAN:END -->
