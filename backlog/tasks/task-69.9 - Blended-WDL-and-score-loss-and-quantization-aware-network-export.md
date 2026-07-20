---
id: TASK-69.9
title: Blended WDL-and-score loss and quantization-aware network export
status: To Do
assignee: []
created_date: '2026-07-20 19:41'
labels:
  - nnue
  - training
  - python
dependencies:
  - TASK-69.8
  - TASK-69.2
parent_task_id: TASK-69
priority: high
ordinal: 111000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the training target and the export path. The loss blends the search score and the game WDL outcome with the lambda schedule from the design contract, so that the network is anchored to real game results and not only to its own predecessor. Implement quantization-aware handling so the values the exporter writes are values the training loop has already seen, then export a network file in the versioned format (TASK-69.2) that the engine loads directly.

The WDL term is the only signal in the whole loop that comes from the rules of chess rather than from the engine evaluation itself, so getting this blend right is what keeps the reinforcement loop anchored to reality.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The loss combines search-score and WDL targets with a configurable, schedulable lambda, covered by a test on a small fixture
- [ ] #2 Training accounts for quantization so exported integer weights reproduce the trained model behaviour within the contract tolerance
- [ ] #3 The exporter writes a versioned network file that the engine loader (TASK-69.2) accepts
<!-- AC:END -->
