---
id: TASK-69.4
title: Scalar quantized NNUE inference wired as a selectable evaluation
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-21 02:02'
labels:
  - nnue
  - inference
dependencies:
  - TASK-69.2
  - TASK-69.3
parent_task_id: TASK-69
priority: high
ordinal: 106000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the portable scalar reference forward pass: from the accumulator (TASK-69.3) through the clipped activation and remaining layers to a single centipawn score, using the exact quantized integer arithmetic from the design contract. Wire it behind Search::evaluate as a selectable evaluation path so the hand-crafted evaluation remains the default until a trained network exists and passes its gate.

This scalar path is the permanent correctness oracle: it is what runs on targets without the SIMD path, and it is the reference the SIMD path (TASK-69.5) and the PyTorch quantized forward (TASK-69.10) are both checked against. Establish the golden-vector test harness here — load (FEN, expected-score) pairs produced alongside a network and assert exact equality — even if seeded initially with a tiny hand-constructed network.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With a loaded network, Search evaluates positions through the scalar quantized forward pass and the evaluation is selectable without disturbing the default hand-crafted path
- [ ] #2 A golden-vector test loads (FEN, expected-score) pairs and asserts exact integer equality against the scalar forward pass
- [ ] #3 The quantized arithmetic (scales, clipping, saturation) matches the design contract and is exercised by tests including near-overflow accumulator states
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine/src/nnue/inference.rs: a pure scalar forward pass forward(network, &Accumulator, side_to_move) -> i32 centipawns implementing the contract's normative arithmetic exactly (concat stm-first, clipped ReLU to [0,QA], i32 output accumulate, i64 multiply-by-SCALE, round-half-away-from-zero divide by QA*QB, clamp to [-10000,10000]). Re-export from nnue/mod.rs.
2. Selection seam: give Search an optional owned Network (default None => hand-crafted, unchanged). When Some, Search::evaluate rebuilds an Accumulator::from_position for the current leaf and runs the forward pass, returning Score::cp already from the side-to-move perspective (no *pov(), forward is stm-relative). Default path byte-for-byte unchanged. Add a constructor/setter to configure it; thread through SearchEngine minimally without disturbing existing call sites.
3. Golden-vector harness: a test loading (FEN, expected-cp) pairs and asserting exact integer equality against the scalar forward pass, seeded with a small hand-constructed network committed as fixture data (FENs + expected scores). Establish the harness structure .10 will reuse.
4. Arithmetic tests: unit tests over the forward pass covering rounding (half-away-from-zero both signs), clipping/saturation at [0,QA], and near-i16-overflow accumulator states; verify the i32/i64 widening prevents overflow before the divide.
5. Scope: from-scratch accumulator at the evaluated leaf (per approved decision); incremental threading + per-node assertion deferred (needs Accumulator API/lifetime change, companion to .5). Document the deferral in notes.
6. Run cargo fmt --check, clippy -D warnings, cargo test --workspace; hand off for review.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 02:02
---
Claiming for implementation on task-69.4-scalar-nnue-inference (worktree /Users/seabo/seaborg-worktrees/task-69.4-scalar-nnue-inference, base 0f73ec8). Integration depth confirmed with the user: from-scratch accumulator at the evaluated leaf; incremental threading deferred.
---
<!-- COMMENTS:END -->
