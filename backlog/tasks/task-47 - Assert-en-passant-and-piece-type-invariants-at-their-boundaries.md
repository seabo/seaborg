---
id: TASK-47
title: Assert en passant and piece-type invariants at their boundaries
status: To Do
assignee: []
created_date: '2026-07-18 18:30'
labels: []
dependencies: []
references:
  - core/src/movegen.rs
  - core/src/position/fen.rs
priority: medium
type: chore
ordinal: 47000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Four places assume an invariant that the surrounding code already establishes, but none of them check it. Each is cheap to close and the group shares one test surface (FEN parsing plus debug assertions exercised by the existing perft suite).

1. core/src/movegen.rs:549 and :644 - assert that ep_square lies on the 6th rank from the moving player perspective, in both the move-generating and the has-any-move paths.
2. core/src/position/fen.rs:451 - reject a FEN whose en passant square does not reconcile with the side to move. This continues the validation work of TASK-11, which made structurally invalid FEN non-panicking but left this semantic check open.
3. core/src/movegen.rs:717 - moves_bb is generic over P: PieceTrait and the PieceType::None arm is statically unreachable, but it is written as a bare panic!() with a TODO. Make the impossibility explicit, either by rejecting it at compile time or with unreachable!() plus a comment stating why it cannot occur.

Assertions on hot paths must be debug-only so release move generation is unaffected.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Move generation debug-asserts the en passant rank invariant in both the generating and has-any-move paths
- [ ] #2 FEN parsing rejects an en passant square inconsistent with the side to move, returning an error rather than panicking
- [ ] #3 A test covers at least one rejected inconsistent-en-passant FEN and one accepted valid one
- [ ] #4 The PieceType::None arm in moves_bb is expressed as a documented impossibility rather than a bare panic with a TODO
- [ ] #5 Added hot-path assertions are debug-only and the movegen and perft benchmarks show no release regression
- [ ] #6 All four TODO comments at the listed sites are removed
<!-- AC:END -->
