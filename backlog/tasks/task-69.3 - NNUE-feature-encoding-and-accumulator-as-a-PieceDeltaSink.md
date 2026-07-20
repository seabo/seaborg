---
id: TASK-69.3
title: NNUE feature encoding and accumulator as a PieceDeltaSink
status: To Do
assignee: []
created_date: '2026-07-20 19:40'
labels:
  - nnue
  - inference
dependencies:
  - TASK-69.1
parent_task_id: TASK-69
priority: high
ordinal: 105000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the input feature indexing from the design contract and the network accumulator that maintains the first-layer activations incrementally, as a new PieceDeltaSink consumer alongside EvalState. This is the core engine integration and the one place a subtle bug would silently cost strength, so it is scoped tightly and validated exactly like the existing incremental evaluation.

The accumulator plugs into the existing seam: Position::replay_last_move_deltas drives add/remove calls, the accumulator is threaded through Search with a push/pop stack for O(1) restore on unmake, and debug builds assert the incremental accumulator against a from-scratch recomputation at every node, reusing the validation pattern already established for EvalState. No forward pass or scoring yet; this task delivers a correct, incrementally-maintained accumulator and its equivalence guarantee.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Feature indices for both perspectives match the design contract and are covered by tests over representative positions
- [ ] #2 The accumulator is maintained incrementally across make and unmake and a debug assertion checks it against a from-scratch recomputation at every node
- [ ] #3 A make-then-unmake restores the accumulator bit-for-bit, and a subtree walk asserts incremental equals from-scratch, mirroring the existing EvalState tests
<!-- AC:END -->
