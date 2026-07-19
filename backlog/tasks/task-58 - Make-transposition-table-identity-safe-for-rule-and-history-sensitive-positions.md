---
id: TASK-58
title: >-
  Make transposition-table identity safe for rule- and history-sensitive
  positions
status: To Do
assignee: []
created_date: '2026-07-19 00:00'
labels:
  - transposition-table
  - zobrist
  - search
  - correctness
  - rules
dependencies: []
references:
  - core/src/position/zobrist.rs
  - core/src/precalc/zobrist.rs
  - engine/src/search.rs
priority: high
type: bug
ordinal: 57000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Zobrist key identifies board state, side to move, castling rights, and en-passant file, but search values also depend on the halfmove clock and potentially on repetition history. Static evaluation is explicitly scaled by the halfmove clock, so identical keys can currently carry different values. Establish and enforce a documented TT-reuse policy for halfmove-clock and repetition-sensitive results. Also canonicalise en-passant hashing so an unusable target does not split positions with identical legal state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A warm-table search cannot reuse a score or bound computed under an incompatible halfmove-clock state
- [ ] #2 The treatment of repetition-dependent results is documented and enforced so history-sensitive draw outcomes cannot be reused as position-intrinsic exact information in an incompatible history
- [ ] #3 Positions that differ only by an en-passant target which cannot affect any legal move have the same canonical transposition identity, while a legally relevant en-passant right remains distinguished
- [ ] #4 Regression tests cover warm-table reuse at materially different halfmove clocks, compatible and incompatible repetition histories, and capturable versus non-capturable en-passant targets
<!-- AC:END -->
