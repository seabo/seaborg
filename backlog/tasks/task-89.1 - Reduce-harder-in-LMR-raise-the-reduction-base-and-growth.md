---
id: TASK-89.1
title: 'Reduce harder in LMR: raise the reduction base and growth'
status: To Do
assignee: []
created_date: '2026-07-27 15:08'
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
