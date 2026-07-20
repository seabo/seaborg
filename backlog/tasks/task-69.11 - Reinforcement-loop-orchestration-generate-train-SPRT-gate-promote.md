---
id: TASK-69.11
title: 'Reinforcement loop orchestration: generate, train, SPRT-gate, promote'
status: To Do
assignee: []
created_date: '2026-07-20 19:42'
labels:
  - nnue
  - training
  - rl
dependencies:
  - TASK-69.4
  - TASK-69.6
  - TASK-69.9
  - TASK-69.10
parent_task_id: TASK-69
priority: high
ordinal: 113000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Automate one turn of the reinforcement loop and the iteration over turns: generate self-play data with the current best network as the evaluator (iteration 0 bootstraps from the hand-crafted evaluation), train the next candidate on it, gate the candidate against the current best with the repository strength-test SPRT harness, and promote it only if it passes. Record attribution for every iteration (data volume, node budget, network id, measured delta) so strength changes stay attributable in the way BENCHMARKS.md and the strength harness require.

This orchestration composes the datagen (TASK-69.6), training/export (TASK-69.9), inference (TASK-69.4), and equivalence (TASK-69.10) pieces; it adds no new numeric machinery, only the loop, the gate, and the bookkeeping.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single command runs one full iteration: generate, train, export, load into the engine, and SPRT-gate the candidate against the current best
- [ ] #2 A candidate is promoted to current-best only when it passes the strength gate, and the decision plus attribution are recorded
- [ ] #3 Iteration 0 bootstraps from the hand-crafted evaluation, and the self-play purity constraint is preserved end to end
<!-- AC:END -->
