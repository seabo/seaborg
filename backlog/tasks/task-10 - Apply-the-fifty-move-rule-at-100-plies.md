---
id: TASK-10
title: Apply the fifty-move rule at 100 plies
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - search
  - rules
dependencies: []
references:
  - engine/src/search.rs
  - core/src/position/mod.rs
priority: high
type: bug
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Position halfmove clock counts plies, but search declares a draw and evaluation reaches zero at 50. Align draw detection and any related evaluation scaling with the chess-rule threshold of 100 halfmoves.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A halfmove clock of 99 does not trigger the fifty-move draw condition
- [ ] #2 A halfmove clock of 100 triggers the draw condition
- [ ] #3 Material evaluation is not incorrectly forced to zero at 50 plies
- [ ] #4 Tests cover FEN positions immediately below, at, and above the threshold
<!-- AC:END -->
