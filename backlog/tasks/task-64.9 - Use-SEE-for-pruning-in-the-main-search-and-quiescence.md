---
id: TASK-64.9
title: Use SEE for pruning in the main search and quiescence
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 05:28'
labels:
  - search
  - pruning
  - see
  - quiescence
dependencies: []
references:
  - engine/src/see.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 72000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Static exchange evaluation is implemented and correct, including the promotion handling added by TASK-49, but it is used only to sort moves. Its two call sites are `MoveLoader::score_captures` (search.rs:1472-1486) and `QMoveLoader::score_captures` (search.rs:1532-1546), both of which assign the SEE value as an ordering score feeding the GoodCaptures, EqualCaptures and BadCaptures phase split. Nothing anywhere uses it to decide not to search a move.

Two applications, delivered together because they share the same predicate and the same measurement:

Quiescence. `QMoveLoader` generates and searches every capture, including those SEE scores as clearly losing, and applies no delta margin. A losing capture near the horizon almost never repays its subtree. Skipping captures with a negative SEE, and skipping captures whose optimistic material gain plus a margin cannot reach alpha, are the two standard cuts and both are absent. Quiescence node share is already instrumented in the telemetry block at search.rs:1341 and should be reported before and after.

Main search. At shallow depth in non-PV nodes, prune captures and quiets whose SEE falls below a depth-scaled threshold. Note that bad captures are currently searched: they are not discarded, only deferred to the BadCaptures phase after quiets (ordering.rs:277-278).

The delta margin compares against the static evaluation and inherits the material-only caveat that applies across this programme. The SEE-based cuts do not: SEE is a material calculation and is unaffected by evaluation quality, which makes this task one of the more reliable gains available before the evaluation work.

TASK-29 covers bounding quiescence recursion by ply. Its second comment records that the large quiescence trees observed in practice are driven by capture and promotion interleaving rather than check evasions, which is exactly what the cuts in this task address; the two are complementary and neither substitutes for the other.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Quiescence skips captures with a losing static exchange evaluation, under a documented threshold
- [ ] #2 Quiescence applies a delta margin so captures that cannot plausibly reach alpha are not searched
- [ ] #3 Neither quiescence cut is applied while in check, where all evasions must remain available
- [ ] #4 The main search prunes moves below a depth-scaled SEE threshold in non-PV nodes at shallow depth
- [ ] #5 Quiescence node counts and the quiescence share of total nodes are reported before and after on a representative position set
- [ ] #6 Tactical test positions requiring a losing capture to find the correct move are covered and still solved
- [ ] #7 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Quiescence cuts in quiesce_inner move loop (not-in-check by construction; evasions path via quiesce_evasions untouched -> AC#3): (a) SEE cut skipping captures with SEE below a documented threshold (0); (b) delta-margin cut skipping captures whose stand-pat + optimistic material gain + margin cannot reach alpha. Add threshold/margin constants.
2. Main-search SEE pruning: new step in the main move loop before make_move; non-PV nodes at shallow depth only, depth-scaled SEE threshold for captures and quiets; guarded by forward_pruning_enabled, move_count>1, node not in check, and best_value not a proven mate so tactics survive. Wire the pre-staged trace.see_skip_node()/see_skipped_nodes() telemetry.
3. Add a #[cfg(test)] see_pruning_disabled toggle + see_pruning_enabled() for isolation tests, mirroring lmr/rfp/futility.
4. Tests: SEE-cut behaviour (losing capture skipped in q-search, delta cut, in-check evasions unaffected); tactical positions requiring a losing/sacrificial capture still solved at fixed depth; q-node share instrumentation.
5. Measure q-node counts and q share before/after on a representative position set; run cargo fmt/clippy/test; run the TASK-27 strength measurement via fastchess directly against a merge-base baseline binary; record all results in implementation notes (AC#5, AC#7).
<!-- SECTION:PLAN:END -->
