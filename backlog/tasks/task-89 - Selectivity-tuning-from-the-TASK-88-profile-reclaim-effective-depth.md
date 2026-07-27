---
id: TASK-89
title: 'Selectivity tuning from the TASK-88 profile: reclaim effective depth'
status: To Do
assignee: []
created_date: '2026-07-27 15:07'
labels:
  - search
  - selectivity
  - strength
dependencies: []
references:
  - engine/src/search.rs
  - BENCHMARKS.md
priority: high
type: feature
ordinal: 151000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 profiled Seaborg's own search and found two first-principles depth-loss sites: late-move reduction is under-reducing (re-search rate 1.6-2.0 percent, mean about 2.1 ply, so ~98 percent of reduced scouts fail low exactly as ordering predicts - reductions are provably conservative), and quiescence is about half the tree. Move ordering (88.8 percent first-move cutoffs) and TT-move availability were measured and explicitly ruled out as loss sites. This umbrella turns TASK-88's ranked experiments into individually-measured tuning changes.

Discipline (see AGENTS.md project philosophy): each child is an independent self-play SPRT at a real time control (10s+0.1s), gated separately so its Elo is attributable; a negative result is a recorded, informative outcome, not a failure. Each child also re-runs the off-by-default selstats profile from TASK-88 to confirm the intended signal moved in the predicted direction - the change is justified by our own measurement, not by matching another engine. All children edit the LMR/quiescence knobs in engine/src/search.rs, so they are serial on the search lane and measured one at a time. These are structural search properties and largely eval-agnostic, so the tuning is expected to transfer to the v2 network.

Recommended order follows TASK-88's leverage-times-cheapness ranking (89.1 first).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each child experiment is completed: retained on a measured non-negative SPRT, or closed with a recorded negative result
- [ ] #2 After the retained changes, the selstats profile is re-run and the net movement in re-search rate, EBF, and quiescence fraction is recorded in BENCHMARKS.md
- [ ] #3 A closing note records which experiments moved strength and which were informative negatives
<!-- AC:END -->
