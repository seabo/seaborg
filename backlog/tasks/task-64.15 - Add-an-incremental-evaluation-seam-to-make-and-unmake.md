---
id: TASK-64.15
title: Add an incremental evaluation seam to make and unmake
status: To Do
assignee: []
created_date: '2026-07-19 13:34'
labels:
  - evaluation
  - nnue
  - architecture
  - performance
dependencies:
  - TASK-64.14
references:
  - core/src/position/mod.rs
  - engine/src/eval.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: enhancement
ordinal: 78000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The evaluation is recomputed from scratch at every call, and there is no hook through which an incrementally maintained evaluation state could be updated. An NNUE accumulator requires one. Establish that seam now, with the tapered hand-crafted evaluation as its first consumer, so that adding a network later is a substitution into an existing mechanism rather than a new mechanism plus a migration.

Current state. `material_evaluation` (eval.rs:32-43) reads bitboards and recomputes on every call. `make_move`, `make_move_unchecked` and `unmake_move` in core maintain incremental state for the Zobrist key already, so the pattern exists in the codebase, but no evaluation state participates in it and there is no place for one to.

Why this is scheduled after the tapered evaluation rather than before. Material and piece-square scores are the canonical incremental terms and are simple enough to validate exhaustively against a from-scratch recomputation. Building the seam against a real consumer establishes the update, undo and correctness-check pattern under conditions where the reference answer is cheap to compute. An NNUE accumulator has the same shape and much more expensive validation.

Design questions to settle and record: where the incremental state lives given that `Search` owns its `Position` by value; how state is restored on unmake, whether by recomputation, a stored delta or a per-ply stack; how null moves interact with it once TASK-50 introduces them, since a null move changes side to move without moving a piece; and how the seam behaves across the copy that occurs when a search is started on a cloned position.

The correctness requirement is absolute and cheap to test here: an incrementally maintained evaluation must equal a from-scratch recomputation at every node. That equivalence should be asserted under debug builds throughout the search, not only in unit tests, because the failure mode is a slow divergence that unit tests on short move sequences will not surface.

This task delivers the seam and the hand-crafted evaluation's use of it. It does not deliver an NNUE accumulator, network format, or inference.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluation state is updated incrementally on make and unmake rather than recomputed from scratch on every evaluate call
- [ ] #2 A debug-build assertion verifies that the incrementally maintained evaluation equals a from-scratch recomputation at every node
- [ ] #3 The restoration strategy on unmake is documented and covered by tests including deep move sequences
- [ ] #4 The interaction with null moves is defined, or the absence of null moves at the time of implementation is recorded together with the constraint it places on TASK-50
- [ ] #5 The seam behaves correctly when a position is cloned to start a search
- [ ] #6 A benchmark records the change in nodes per second against the from-scratch baseline
- [ ] #7 The design is documented sufficiently that an NNUE accumulator can be added as a further consumer without reworking the seam
<!-- AC:END -->
