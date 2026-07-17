---
id: TASK-5
title: Seal chess domain safety boundaries
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - safety
  - core
dependencies: []
references:
  - core/src/position/square.rs
  - core/src/position/board.rs
  - core/src/mov.rs
  - core/src/position/mod.rs
priority: high
type: bug
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Public safe domain types currently allow invalid squares, moves, and positions to reach unchecked indexing and mutation paths. Make invalid state construction explicit and ensure safe APIs cannot cause undefined behavior from caller-controlled values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Invalid square values cannot be constructed through the normal safe API
- [ ] #2 Safe Board, Move, and Position operations reject invalid input without undefined behavior in debug or release builds
- [ ] #3 Any remaining unchecked operations are private or exposed through an unsafe API with a precise safety contract
- [ ] #4 Regression tests cover invalid square, move, and blank-position inputs
<!-- AC:END -->
