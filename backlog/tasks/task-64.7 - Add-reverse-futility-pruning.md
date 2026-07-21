---
id: TASK-64.7
title: Add reverse futility pruning
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 01:43'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add REVERSE_FUTILITY_MAX_DEPTH (6) and reverse_futility_margin(depth) constants next to razoring.
2. In the interior-node path, right after razoring (Step 7), add reverse futility pruning: in non-PV nodes, not in check, depth <= max, beta.is_cp(), when eval - margin(depth) >= beta, return eval (fail-high without generating a move). Mirror image of razoring on the beta side.
3. Add a #[cfg(test)] rfp_disabled toggle (mirroring lmr_disabled) so the guard-soundness test can isolate RFP.
4. Tests: (a) RFP-on vs RFP-off returns identical score/best move on decisive/mate positions where guards keep it sound; (b) RFP reduces node count on a quiet position where it fires; (c) unit tests for the margin/guard helper.
5. Run required checks; measure with TASK-27 strength-regression script and record result (incl. null/negative) in implementation notes.
<!-- SECTION:PLAN:END -->
