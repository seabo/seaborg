---
id: TASK-89.1
title: 'Reduce harder in LMR: raise the reduction base and growth'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-27 15:08'
updated_date: '2026-07-27 16:46'
labels:
  - search
  - selectivity
  - strength
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-89
priority: high
type: feature
ordinal: 152000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 1 (top-ranked). Mechanism: raise the reduction so the mean climbs above ~2.1 ply and the LMR re-search rate rises from ~1.7 percent toward a healthier band; fewer nodes per subtree buys more iterative-deepening depth at equal time. The same ordering that yields an 88.8 percent first-move-cutoff rate is trustworthy enough to reduce the tail harder. Tunables: LMR_BASE, LMR_DIVISOR in engine/src/search.rs. Risk: if strength drops, the reductions were already near-optimal - an informative result in itself.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 LMR_BASE and/or LMR_DIVISOR are swept; the chosen values are measured by self-play SPRT at 10s+0.1s against the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms the re-search rate rose toward a healthier band and mean reduction increased, without the re-search rate exploding
- [ ] #3 Retained only on a non-negative SPRT; a strength drop is recorded as an informative negative (reductions already near-optimal) and reverted
- [ ] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Build a baseline selstats binary at the branch base and measure the pre-change LMR re-search rate and mean reduction with tools/diag/selectivity_profile.py (fixed depth 14 + 2M nodes, bench-positions.epd, 64MB hash), reproducing the TASK-88 reading (~1.7%, ~2.1 ply).
2. Sweep LMR_BASE / LMR_DIVISOR over a small grid (raise base and/or steepen growth by lowering the divisor). For each candidate, rebuild a selstats binary and measure re-search rate + mean reduction. Select the candidate that lifts the mean reduction above ~2.1 ply and the re-search rate toward a healthier band without exploding it.
3. Apply the chosen constants to engine/src/search.rs (default build path).
4. Build optimized release binaries at the base (baseline) and with the chosen constants (candidate); run an authoritative self-play SPRT at tc=10+0.1 via tools/strength/strength_test.py, concurrency scaled to the P-cores.
5. Re-run the selstats profile on the retained candidate to confirm AC#2: re-search rate rose and mean reduction increased without the re-search rate exploding.
6. Retain only on a non-negative SPRT; on a strength drop, revert the constants and record the informative negative (reductions already near-optimal). Record the sweep, chosen values, SPRT verdict, and profile movement in BENCHMARKS.md.
7. Run the repo-required checks (fmt/clippy --all-features/test) and hand off to review.
<!-- SECTION:PLAN:END -->
