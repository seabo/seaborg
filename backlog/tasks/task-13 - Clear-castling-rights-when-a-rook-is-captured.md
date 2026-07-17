---
id: TASK-13
title: Clear castling rights when a rook is captured
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - core
  - movegen
  - rules
dependencies: []
references:
  - core/src/position/mod.rs
  - core/src/position/castling.rs
  - core/src/movegen.rs
priority: high
type: bug
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Position updates castling rights only from the moving piece origin. Capturing a rook on its starting square can leave stale rights that may later permit illegal castling.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Capturing a rook on a1, h1, a8, or h8 clears the corresponding castling right
- [ ] #2 Move generation requires the correctly colored king and rook on their castling origin squares
- [ ] #3 Make and unmake restore castling rights and Zobrist keys exactly across rook captures
- [ ] #4 Tests cover all four rook-capture squares and stale-right FEN inputs
<!-- AC:END -->
