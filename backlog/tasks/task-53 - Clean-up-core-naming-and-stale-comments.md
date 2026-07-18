---
id: TASK-53
title: Clean up core naming and stale comments
status: To Do
assignee: []
created_date: '2026-07-18 19:38'
labels: []
dependencies:
  - TASK-48
references:
  - core/src/bb.rs
  - core/src/position/mod.rs
  - core/src/macros.rs
priority: low
type: chore
ordinal: 53000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Seven small items in core, none individually worth a review cycle. Grouped deliberately: seven trivial diffs reviewed together cost far less than seven review rounds.

1. core/src/bb.rs:48 - Bitboard::new(u64) should become a From impl or be deleted. Decide which; 18 call sites.
2. core/src/position/board.rs:16 - rename Board::new to Board::empty, which is what it does. 3 call sites.
3. core/src/position/mod.rs:136 - rename the State struct to describe its contents (pins, checks, blockers).
4. core/src/position/mod.rs:108 - stale TODO asking to remove pub from Position fields; they are already pub(crate). Delete the comment.
5. core/src/position/mod.rs:134 - open question about wrapping State in an Arc as Pleco does. The answer is no: Position is cloned rarely and State is small. Delete the comment, record the reasoning.
6. core/src/macros.rs:67 - TODO to drive Bitboard bit operations through the impl_bit_ops macro. Do not do this; the manual impls are where Bitboard type safety lives, and macro-generalising them works against TASK-48. Delete the comment, record the decision.
7. core/src/position/piece.rs:54 - Piece::player() could be one arithmetic op instead of a 13-arm match. Only change this if a benchmark shows a real gain; the match very likely already compiles to a jump table. Leaving the code as-is and removing the TODO is an acceptable outcome.

Gated on TASK-48 because both touch bb.rs and position/mod.rs signatures; doing the renames first would conflict with the typed-accessor work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bitboard::new is either replaced by a From impl or removed, with all call sites migrated
- [ ] #2 Board::new is renamed to Board::empty with all call sites migrated
- [ ] #3 The State struct is renamed to describe its contents
- [ ] #4 The stale pub-fields, Arc-wrapping and impl_bit_ops TODO comments are deleted, with the Arc and impl_bit_ops reasoning recorded in the implementation notes
- [ ] #5 Piece::player is either optimised with benchmark evidence of a gain, or left unchanged with the TODO removed and the benchmark result recorded
- [ ] #6 No behaviour changes: the full test suite passes unchanged and the perft benchmarks show no regression
<!-- AC:END -->
