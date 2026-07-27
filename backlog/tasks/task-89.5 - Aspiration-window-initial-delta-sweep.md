---
id: TASK-89.5
title: Aspiration window initial-delta sweep
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
priority: low
type: feature
ordinal: 156000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 5 (lowest expected payoff - the 12-13 percent per-window re-search rate is already modest - but a cheap parameter sweep). Tunable: ASPIRATION_INITIAL_DELTA in engine/src/search.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ASPIRATION_INITIAL_DELTA is swept; the chosen value is measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms the aspiration re-search rate moved as expected
- [ ] #3 Retained only on a non-negative SPRT; a negative is recorded and reverted; conclusion from Seaborg's own signals and self-play
<!-- AC:END -->
