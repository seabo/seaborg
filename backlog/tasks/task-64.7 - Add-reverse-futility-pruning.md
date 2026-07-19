---
id: TASK-64.7
title: Add reverse futility pruning
status: To Do
assignee: []
created_date: '2026-07-19 13:32'
labels:
  - search
  - pruning
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 70000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add reverse futility pruning, also called static null move pruning: in a non-PV node near the horizon, when the static evaluation exceeds beta by a depth-scaled margin, return without searching.

This is distinct from the forward futility pruning tracked by TASK-50, which skips individual quiet moves whose evaluation plus a margin cannot reach alpha. Reverse futility prunes the whole node on the opposite side of the window, before any move is generated. The two are frequently confused and are separately worth having; TASK-50 should not be treated as covering this.

It is placed alongside the existing razoring at search.rs:768, which is its mirror image on the alpha side, and shares the same guard conditions: not in check, non-PV node, shallow remaining depth, and a beta that is not a mate score.

Caveat. This decides what to discard by comparing a static evaluation against a margin, and `Search::evaluate` (search.rs:1096) is material-only. The margin is therefore being applied to a signal that ignores king safety, piece activity and pawn structure entirely. A gain is not guaranteed before the evaluation work lands, and a null or negative measurement here is itself useful evidence about evaluation quality and should be recorded rather than worked around by margin tuning.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reverse futility pruning is applied in non-PV nodes below a documented depth and is disabled in check and when beta is a mate score
- [ ] #2 The technique is implemented separately from and does not duplicate the forward futility pruning of TASK-50
- [ ] #3 A fixed-depth search on a position set where the guards are inactive returns unchanged best moves, confirming the guards
- [ ] #4 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes, including a null or negative result and its bearing on evaluation quality
<!-- AC:END -->
