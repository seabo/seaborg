---
id: TASK-69.3
title: NNUE feature encoding and accumulator as a PieceDeltaSink
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-20 22:58'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine/src/nnue module (sibling of eval), declared in lib.rs.
2. Feature encoding: INPUT_DIM=768, feature_index(perspective, piece, square) = relative_square ^ + 64*pt0 + 384*side, per the design contract. Unit tests over representative pieces/squares/perspectives (both colours, friendly/enemy, orientation flip).
3. FeatureTransformer: in-memory i16 weight table (input_dim x H feature-major) + i16 bias, parameterizable H (multiple of 16). Minimal container the accumulator needs; the file loader (TASK-69.2) will construct it later.
4. Accumulator: two per-perspective i16 activation vectors seeded from bias; implements PieceDeltaSink (add/remove toggle one feature column per perspective). from_position rebuild is the from-scratch reference, mirroring EvalState::from_position.
5. Tests mirroring EvalState: subtree walk asserting incremental == from-scratch at every node (make and unmake) across captures/castling/en-passant/promotions; make-then-unmake bit-for-bit restore; clone equivalence. Use a deterministic synthetic FeatureTransformer with bounded weights.
6. Run cargo fmt --check, clippy -D warnings, cargo test --workspace. Hand off for review. No forward pass/scoring and no Search wiring (deferred to TASK-69.4).
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 22:58
---
Claiming for implementation on task-69.3-nnue-accumulator.
---
<!-- COMMENTS:END -->
