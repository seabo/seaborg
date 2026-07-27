---
id: TASK-89.2
title: 'Engage LMR earlier: lower the move threshold'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-27 15:08'
updated_date: '2026-07-27 21:53'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Selstats sweep of LMR_MOVE_THRESHOLD candidate values (3 -> 2, 3 -> 1) with tools/diag/selectivity_profile.py at fixed depth 14 and fixed 2M nodes over bench-positions.epd. Confirm the intended signal: engaging LMR earlier raises the mean reduction / shifts the reduction distribution and lowers EBF (more depth per node) without the re-search rate exploding. Pick the candidate that best trades a modest re-search-rate rise for lower EBF.
2. Build the target-cpu=native release for the baseline (threshold=3) and the chosen candidate; record binary sha256 for each.
3. Self-play SPRT at tc=10+0.1 (64 MB hash, 1 worker/engine) with tools/strength/strength_test.py, elo0=-5 elo1=0 (no-regression gate), mirroring the TASK-89.1 methodology.
4. Record the sweep, profile movement, and SPRT verdict in BENCHMARKS.md as a new 'Engage LMR earlier' subsection.
5. Retain only on a measured non-negative SPRT; a negative is recorded and the constant reverted to 3. No cross-engine diffing; conclusion from Seaborg's own signals + self-play.
<!-- SECTION:PLAN:END -->
