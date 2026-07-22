---
id: TASK-69.12
title: Run the bootstrap programme and measure per-iteration strength
status: To Do
assignee: []
created_date: '2026-07-20 19:42'
updated_date: '2026-07-22 02:55'
labels:
  - nnue
  - rl
  - strength
dependencies:
  - TASK-69.11
  - TASK-69.5
  - TASK-64.22
parent_task_id: TASK-69
priority: medium
ordinal: 114000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Execute the reinforcement loop (TASK-69.11) for the initial programme of iterations using the SIMD inference path (TASK-69.5) for throughput, and record the outcome. Measure strength after each iteration against both the previous best and, where feasible, an external fixed reference via the existing gauntlet harness, so the curve is anchored to an absolute scale and not only to self-play deltas. Capture the realised datagen throughput and training cost against the earlier estimates, and record where the strength curve begins to flatten.

The deliverable is evidence: the trained network that becomes the new default evaluation, plus a recorded strength curve and cost accounting for the programme.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The loop runs for the planned iterations and produces a network that passes its strength gate and becomes the default evaluation
- [ ] #2 Per-iteration strength is recorded against the previous best, and against an external reference where feasible, with results archived per the strength-testing docs
- [ ] #3 Realised datagen throughput and training cost are recorded and compared against the pre-run estimates
<!-- AC:END -->
