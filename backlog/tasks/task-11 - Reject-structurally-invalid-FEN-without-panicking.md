---
id: TASK-11
title: Reject structurally invalid FEN without panicking
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - core
  - fen
  - input
dependencies: []
references:
  - core/src/position/fen.rs
  - core/src/position/state.rs
priority: high
type: bug
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FEN parsing can accept ranks with the wrong width and can construct positions without the kings required by State initialization, turning invalid input into panics or invalid internal state. Validate structural invariants before Position construction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every FEN rank must represent exactly eight squares, including the final rank
- [ ] #2 Missing, duplicate, or otherwise structurally invalid king data returns FenError rather than panicking
- [ ] #3 Invalid structural input never reaches State or Zobrist initialization
- [ ] #4 Regression tests cover short and long final ranks, empty boards, missing kings, and duplicate kings
<!-- AC:END -->
