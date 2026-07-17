---
id: TASK-13
title: Clear castling rights when a rook is captured
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 21:39'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Update castling-right transitions during make_move to account for both the moving origin and a captured rook origin square, preserving incremental Zobrist updates and undo snapshots.
2. Harden castling generation so the side-to-move king and same-colored rook must occupy their exact origin squares before a castle is emitted.
3. Add focused position and move-generation regression tests for captures on a1, h1, a8, and h8, exact make/unmake state and Zobrist restoration, and stale-right FENs with missing or wrong-colored origin pieces.
4. Run focused tests, cargo fmt --check, and cargo test --workspace; commit implementation and prepare the immutable review handoff.
<!-- SECTION:PLAN:END -->
