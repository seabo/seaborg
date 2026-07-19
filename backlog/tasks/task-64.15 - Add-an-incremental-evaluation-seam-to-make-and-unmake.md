---
id: TASK-64.15
title: Add an incremental evaluation seam to make and unmake
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 13:34'
updated_date: '2026-07-19 21:55'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a piece-delta seam to core: a `PieceDeltaSink` trait (add/remove piece at square) and `Position::replay_last_move_deltas`, which replays the board changes of the most recently made move onto a sink using core's existing move geometry (capture, en passant, castling, promotion). This keeps evaluation weights out of core and gives the future NNUE accumulator the same entry point.
2. Add `EvalState` to engine::eval: a White-relative { mg, eg, phase } accumulator with add_piece/remove_piece primitives (sharing the exact PST arithmetic), `from_position` (from-scratch reference), and `score()` (tapered interpolation). Implement `PieceDeltaSink` for it. Refactor `tapered_evaluation` to `EvalState::from_position(pos).score()` so there is one source of truth.
3. Wire the seam into Search: hold `eval_state: EvalState` (rebuilt from the position in `build`, so a cloned start position is correct by construction) plus an `eval_stack` for O(1) restore. Add make/unmake wrappers that update the accumulator incrementally on make and restore it from the stack on unmake, and replace the raw pos.make/unmake call sites. `evaluate()` reads `eval_state.score()`.
4. Correctness: a debug assertion after every make compares the incremental accumulator to a full from-scratch recomputation (fires at every node, not only in unit tests); `evaluate()` asserts the consumed score equals the from-scratch score.
5. Null moves: none exist yet (TASK-50). Document that a null move moves no piece, so the White-relative accumulator is unchanged across it and must simply be saved/restored; record this constraint for TASK-50.
6. Tests: deep move-sequence make/unmake round-trip equivalence, cloned-position correctness, and per-node incremental==from-scratch over a real search.
7. Benchmark: round-robin master vs branch on the search benches; record the NPS change and attribution.
<!-- SECTION:PLAN:END -->
