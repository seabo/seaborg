---
id: TASK-69.4
title: Scalar quantized NNUE inference wired as a selectable evaluation
status: To Do
assignee: []
created_date: '2026-07-20 19:40'
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
