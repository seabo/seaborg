---
id: TASK-86.1
title: Wire the NNUE accumulator to update incrementally in the search hot loop
status: To Do
assignee: []
created_date: '2026-07-25 12:22'
labels:
  - nnue
dependencies: []
parent_task_id: TASK-86
priority: high
ordinal: 143000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Today Search::evaluate rebuilds the accumulator from scratch at every evaluated leaf via Accumulator::from_position (engine/src/search.rs), an O(pieces x H) cost per node. The incremental PieceDeltaSink accumulator (TASK-69.3) and the make/unmake evaluation seam (TASK-64.15) already exist but are not wired into the search hot loop. Maintain the accumulator incrementally along make/unmake through search so per-node eval cost becomes O(features-toggled x H) and is nearly independent of piece count. This is a pure speed change with bit-identical eval output, and it is the prerequisite that makes a wider or king-bucketed network affordable without cratering NPS; the payoff grows with hidden width H.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accumulator is maintained incrementally across make/unmake during search (e.g. a per-ply accumulator stack mirroring the EvalState eval_stack), and read at Search::evaluate without a from-scratch rebuild
- [ ] #2 Null moves and any other move types that do not change piece placement are handled correctly (accumulator unchanged; only side-to-move / concatenation order flips)
- [ ] #3 Accumulator::from_position is retained as a from-scratch equivalence reference, and a test asserts the incrementally-maintained accumulator is bit-identical to a full rebuild across a representative position/move suite including captures, promotions, castling, en passant, and null moves
- [ ] #4 Evaluation output is bit-identical before and after this change (verified by a fixed-nodes search producing identical results)
- [ ] #5 Before/after single-thread bench NPS is measured under controlled conditions and recorded in BENCHMARKS.md with attribution
<!-- AC:END -->
