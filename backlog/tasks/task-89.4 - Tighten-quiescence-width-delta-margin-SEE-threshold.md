---
id: TASK-89.4
title: 'Tighten quiescence width: delta margin / SEE threshold'
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
ordinal: 155000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 4. Mechanism: quiescence is ~half the tree; a tighter QUIESCENCE_DELTA_MARGIN or SEE cut trims the leaf explosion, converting quiescence nodes into main-search plies. IMPORTANT: this tunes the EXISTING, already net-positive quiescence SEE prune - it does NOT reintroduce a main-search SEE prune, which measured net-harmful (TASK-64.21, closed negative). Tunable: QUIESCENCE_DELTA_MARGIN in engine/src/search.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 QUIESCENCE_DELTA_MARGIN (and/or the quiescence SEE threshold) is tightened; measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms the quiescence fraction of all nodes fell
- [ ] #3 No main-search SEE prune is introduced; only the existing quiescence prune is tuned
- [ ] #4 Retained only on a non-negative SPRT; a negative is recorded and reverted; conclusion from Seaborg's own signals and self-play
<!-- AC:END -->
