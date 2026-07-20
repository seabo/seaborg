---
id: TASK-69.10
title: Golden-vector emission and three-way differential equivalence test
status: To Do
assignee: []
created_date: '2026-07-20 19:41'
labels:
  - nnue
  - training
  - inference
dependencies:
  - TASK-69.9
  - TASK-69.4
parent_task_id: TASK-69
priority: high
ordinal: 112000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Close the Rust/Python sync loop. The export step (TASK-69.9) also emits a golden-vector set of (FEN, expected-score) pairs spanning tactical, endgame, king-safety, and near-overflow positions. A test asserts three-way equality on that set: the PyTorch quantized forward pass, the Rust scalar forward pass (TASK-69.4), and — when present — the Rust SIMD forward pass (TASK-69.5) all produce the same integer scores.

This is the entire cross-language sync guarantee. With it, keeping the two implementations in step is a solved problem; without it, drift is a recurring, hard-to-localize source of lost strength.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Export emits a golden-vector set covering tactical, endgame, king-safety, and near-overflow positions
- [ ] #2 A differential test asserts PyTorch quantized forward equals the Rust scalar forward exactly over the golden vectors
- [ ] #3 When the SIMD path is available it is included in the equality assertion, giving a three-way check
<!-- AC:END -->
