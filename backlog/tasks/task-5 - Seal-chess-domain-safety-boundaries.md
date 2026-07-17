---
id: TASK-5
title: Seal chess domain safety boundaries
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:02'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make `Square` representation private, add checked public constructors/conversions and explicit crate-internal construction for proven indices, then migrate callers.
2. Harden `Move` construction and board access so safe inputs cannot reach unchecked indexing or inconsistent move encodings.
3. Validate `Position::make_move` before mutation, including null, empty-origin, ownership, destination, and special-move invariants; add regression coverage for blank positions.
4. Run focused tests, `cargo fmt --check`, and `cargo test --workspace`; commit implementation and create the immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sealed `Square` behind a private external representation with checked raw conversion and checked arithmetic. Hardened Board placement, Move construction, and pre-mutation Position move validation. Migrated engine consumers to the public square index accessor and added debug/release regression coverage for invalid square, board, move, null-move, and blank-position inputs.

Workspace verification note: `cargo test --workspace` passed all core tests and 36/37 non-ignored engine tests; pre-existing `engine::tt::tests::gen_bound` fails because the test deliberately passes generation 64 to a function with `debug_assert!(gen < 64)`. The same contradictory test and assertion are present at the base commit.
<!-- SECTION:NOTES:END -->
