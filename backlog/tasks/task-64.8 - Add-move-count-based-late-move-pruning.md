---
id: TASK-64.8
title: Add move-count based late move pruning
status: To Do
assignee: []
created_date: '2026-07-19 13:32'
labels:
  - search
  - pruning
dependencies:
  - TASK-64.2
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 71000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In non-PV nodes near the horizon, stop searching quiet moves once a depth-indexed move count has been exceeded. The move loop at search.rs:803-910 currently searches every generated move at every node regardless of how late in the ordering it appears.

Late move pruning is the cheapest of the move-loop prunings because it consults only the move counter and the depth, not the evaluation. That makes it the one pruning technique in this programme whose effectiveness does not depend on the quality of the static evaluation, and therefore the one most likely to show a clean gain before the evaluation work lands.

Its effectiveness does depend on move ordering, since discarding late moves is only safe when late genuinely means unpromising. It is gated on the history heuristic being active, because quiet moves are presently ordered by an all-zero table and there is no sense in which a quiet move is currently late.

A move counter (`move_count`, search.rs:800) already exists and is incremented per move. The staged ordering makes the phase of the current move available via `OrderedMoves::phase` (ordering.rs:540), which distinguishes quiets from captures and killers without inspecting the move.

Two adjacent items worth folding in while this code is being touched. First, the ordering's final phase is Underpromotions (ordering.rs:279), synthesised from the queen-promotion segment; most engines exclude these outside quiescence and the saving is free. Second, whether pruning should also apply to bad captures, which sit after quiets in the phase order, is worth settling and recording.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Quiet moves beyond a depth-indexed move count are not searched in non-PV nodes, and the technique is disabled in check and in PV nodes
- [ ] #2 The threshold is documented and its interaction with history-based ordering is stated
- [ ] #3 A decision on whether underpromotions are searched outside quiescence is recorded and implemented
- [ ] #4 A decision on whether bad captures are subject to the same pruning is recorded
- [ ] #5 Node counts at fixed depth are reduced on a representative position set, with figures recorded in the implementation notes
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
