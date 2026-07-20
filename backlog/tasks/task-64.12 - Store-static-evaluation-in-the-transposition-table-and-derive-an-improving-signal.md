---
id: TASK-64.12
title: >-
  Store static evaluation in the transposition table and derive an improving
  signal
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 13:33'
updated_date: '2026-07-20 15:11'
labels:
  - search
  - transposition-table
  - pruning
dependencies:
  - TASK-57
  - TASK-64.1
references:
  - engine/src/tt.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 75000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The transposition table stores no static evaluation, and the search has no notion of whether a line is improving. Both are prerequisites for tuning the margins used by the pruning techniques in this programme.

Current state. `Entry` (tt.rs:199-205) packs a signature, depth, generation-and-bound byte, score and move into exactly eight bytes with no eval field. `Search::evaluate` (search.rs:1095-1097) recomputes from bitboards on every call and the result is stored nowhere, so a re-visited position pays for its evaluation again, and a node has no access to the evaluation of the position two plies above it.

The improving signal is the standard derived quantity: comparing the static evaluation at the current ply against the evaluation two plies earlier tells the search whether the side to move is doing better than it was, and every margin-based pruning decision is conventionally widened or narrowed on that basis. Reverse futility, futility, late move pruning and reduction amounts all read it. Without it, each of those techniques applies the same margin to a position that is collapsing and one that is consolidating.

This depends on the transposition-table rewrite because adding a field to the current entry is not possible: the entry is exactly eight bytes with every bit assigned, which is a constraint TASK-57 removes by design. It depends on the search-stack refactor because the improving comparison reads the evaluation at ply minus two, which is per-ply state that stack introduces.

A design question to settle: whether a stored evaluation should be trusted after a signature-verified hit, given that the score and evaluation have different soundness requirements. The score is subject to the fifty-move and repetition gating documented at search.rs:418-448, while a position-intrinsic static evaluation is not, since `evaluate` deliberately does not read the clock.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The transposition table stores a static evaluation alongside the score, within the layout defined by TASK-57
- [ ] #2 A verified hit supplies the static evaluation and avoids recomputation, and the soundness argument for reusing it is documented alongside the existing score-reuse rules
- [ ] #3 An improving signal derived from the evaluation two plies earlier is available at every node
- [ ] #4 At least one existing margin-based pruning technique consumes the improving signal
- [ ] #5 A test asserts the improving signal is correct across a sequence where the evaluation rises and then falls
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. tt.rs: consume the 15 reserved data-word bits for a static-eval field (bits 48..63). Store the eval as Option<Score>: a 15-bit two's-complement centipawn value, with a dedicated sentinel for 'no eval'. Update pack/Snapshot::from_data, Snapshot::eval(), Table::store signature, layout docs, and the reserved-bit invariant. Add round-trip/none-sentinel tests.
2. search.rs: capture the probed entry's stored eval before the entry is consumed. In Step 6 use the verified hit's stored eval instead of recomputing (position-intrinsic, so trusted from any full-key hit regardless of the clock gate); document the soundness argument beside the existing score-reuse rules. Pass the node's eval to the Step 24 store and to store_quiescence.
3. Improving signal: add a pure is_improving(current, two_plies_ago) helper plus eval_two_plies_ago(ply), computed at every main-search node from the per-ply stack. Feed it into razoring (the one existing margin-based technique) by widening the razor margin when improving.
4. Tests: unit-test is_improving across a rising-then-falling eval sequence; add tt eval round-trip and none tests; keep quiescence/search tt tests compiling with the new store signature.
5. Run cargo fmt/clippy/test and the TASK-27 strength-regression script; record results in notes.
<!-- SECTION:PLAN:END -->
