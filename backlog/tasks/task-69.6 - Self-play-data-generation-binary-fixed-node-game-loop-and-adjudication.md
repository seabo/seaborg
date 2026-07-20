---
id: TASK-69.6
title: 'Self-play data generation binary: fixed-node game loop and adjudication'
status: To Do
assignee: []
created_date: '2026-07-20 19:41'
labels:
  - nnue
  - datagen
dependencies:
  - TASK-69.1
parent_task_id: TASK-69
priority: high
ordinal: 108000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build the self-play data generation binary that plays games against itself at a fixed, low node budget per move (reusing the node-count search limit from TASK-64.6) and runs many games in parallel across cores, one single-threaded search per game for throughput. Iteration 0 uses the existing hand-crafted evaluation, so this binary does not depend on NNUE inference and can be developed in parallel with the inference track; a later switch selects the current network as the evaluator.

Each game records, per retained position, the search score and the eventual game outcome, and adjudicates results (win, draw, loss) with clear resign and draw rules. This task owns the game loop, parallel orchestration, and adjudication; the on-disk sample format and position filtering are TASK-69.7.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The binary self-plays games at a configurable fixed node budget per move and runs games concurrently across a configurable number of workers
- [ ] #2 Games terminate by mate, stalemate, draw rules, or adjudication, and each recorded position carries a search score and the final game outcome
- [ ] #3 Throughput (positions per second, aggregate) is measured and recorded so the training-cost estimates can be validated against reality
<!-- AC:END -->
