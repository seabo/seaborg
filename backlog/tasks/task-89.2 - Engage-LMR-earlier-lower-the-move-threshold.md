---
id: TASK-89.2
title: 'Engage LMR earlier: lower the move threshold'
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
priority: medium
type: feature
ordinal: 153000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 2. Mechanism: apply the 'ordering is trustworthy' argument to the front of the move tail - start reducing before the current threshold and/or reduce the first few non-PV moves, claiming depth on moves ordering already ranks low. Cheap; pairs naturally with 89.1 but is gated separately to attribute the gain. Tunable: LMR_MOVE_THRESHOLD in engine/src/search.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 LMR_MOVE_THRESHOLD (and/or first-few-non-PV reduction) is swept; chosen value measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms depth reached / EBF improved without the re-search rate exploding
- [ ] #3 Retained only on a non-negative SPRT; a negative is recorded and reverted
- [ ] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->
