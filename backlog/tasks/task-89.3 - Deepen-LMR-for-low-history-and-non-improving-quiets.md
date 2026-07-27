---
id: TASK-89.3
title: Deepen LMR for low-history and non-improving quiets
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
ordinal: 154000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 3. Mechanism: the reduction distribution has almost no mass beyond 3 ply; widen the history-and-improving modulation so the least-promising late quiets are cut harder while the trusted prefix keeps its depth. Signal: the reduction-ply distribution shifts right without the re-search rate exploding.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The history/improving reduction modulation is widened; the change is measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms the reduction-ply distribution shifts right (more mass beyond 3 ply) without the re-search rate exploding
- [ ] #3 Retained only on a non-negative SPRT; a negative is recorded and reverted
- [ ] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->
