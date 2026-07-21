---
id: TASK-64.8
title: Add move-count based late move pruning
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 15:21'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a move-count late move pruning (LMP) step to the main search move loop (search.rs ~1583). Gate on: non-PV node, not in check, current move is in the history-ordered Quiet phase, remaining depth <= LMP_MAX_DEPTH, and move_count already searched >= a depth-indexed threshold late_move_count(depth). When it fires, break the remaining quiets for the node (bad captures still follow). The move counter (move_count) and OrderedMoves::phase already exist; no eval consulted.
2. Document the threshold constant/formula and state that LMP safety rests on history-based quiet ordering (activated by TASK-64.2), which is now always on.
3. Decision (AC#3): exclude underpromotions from the main search. They are the final ordering phase, always derived from a queen promotion that IS searched, so dropping them never removes the last legal move; keep them in quiescence. Implement by breaking the main move loop when the Underpromotions phase is reached.
4. Decision (AC#4): do NOT subject bad captures to LMP. Move-count pruning of losing captures is effectively main-search SEE pruning, which measured as a strong Elo regression previously; bad captures can still be tactical sacrifices. Record rationale in code + notes.
5. Add an lmp_enabled/lmp_disabled test hook mirroring lmr/futility hooks.
6. Tests: LMP shrinks the tree; LMP does not change a sound fixed-depth result vs disabled; underpromotions are excluded from the main search but still yielded in quiescence.
7. Measure node-count reduction at fixed depth on a representative set and run the TASK-27 strength script (fastchess nodes-limited match vs merge-base). Record figures in implementation notes.
<!-- SECTION:PLAN:END -->
