---
id: TASK-69.7
title: 'Packed training-sample format, position filtering, and opening diversification'
status: To Do
assignee: []
created_date: '2026-07-20 19:41'
labels:
  - nnue
  - datagen
dependencies:
  - TASK-69.6
parent_task_id: TASK-69
priority: high
ordinal: 109000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define and implement the compact on-disk sample format the data generator writes and the trainer reads: a packed position plus the search score plus the WDL outcome, sized for streaming hundreds of millions of samples. Add the position filtering that determines which positions are retained (for example skipping positions in check, positions whose best move is a capture, and early opening plies) and the opening diversification that keeps the game distribution broad (randomized opening plies or an internally-generated opening set, without importing external game data, to honour the purity constraint).

Format and filtering are separated from the game loop (TASK-69.6) so the encoding can be reviewed and versioned on its own; it is a data contract the Python dataloader (TASK-69.8) depends on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented packed sample format encodes position, search score, and WDL outcome, and round-trips through a reader and writer with tests
- [ ] #2 Position filtering rules are implemented and configurable, with tests asserting filtered categories are excluded
- [ ] #3 Opening diversification broadens the starting-position distribution using only internally-generated data, with no external game or position files consumed
<!-- AC:END -->
