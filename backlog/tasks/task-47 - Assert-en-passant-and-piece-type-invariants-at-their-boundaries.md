---
id: TASK-47
title: Assert en passant and piece-type invariants at their boundaries
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-19 20:20'
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
- [x] #1 Move generation debug-asserts the en passant rank invariant in both the generating and has-any-move paths
- [x] #2 FEN parsing rejects an en passant square inconsistent with the side to move, returning an error rather than panicking
- [x] #3 A test covers at least one rejected inconsistent-en-passant FEN and one accepted valid one
- [x] #4 The PieceType::None arm in moves_bb is expressed as a documented impossibility rather than a bare panic with a TODO
- [x] #5 Added hot-path assertions are debug-only and the movegen and perft benchmarks show no release regression
- [x] #6 All four TODO comments at the listed sites are removed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. movegen.rs generate + has-any-move ep paths: replace the two TODOs with a debug_assert_eq! that ep_square's rank equals PL::player().relative_rank(5) (the 6th rank from the mover's perspective). Debug-only so release movegen is unaffected.
2. fen.rs parse_ep_square: take the side to move, parse the square, then reject (FenError) any square whose rank does not match turn.relative_rank(5) — 6th rank for White to move, 3rd for Black. Update the from_fen call site to pass turn. Remove the TODO.
3. movegen.rs moves_bb: replace the PieceType::None bare panic!()+TODO with unreachable!() plus a comment explaining no PieceTrait impl produces None, so the arm is statically unreachable.
4. Add fen tests: at least one rejected inconsistent-ep FEN and one accepted valid one.
5. Run fmt/clippy/test; confirm no release movegen/perft regression (assertions are debug-only).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation decisions:
- Movegen ep-rank invariant expressed with debug_assert_eq! comparing ep_square.rank_idx_of_sq() to PL::player().relative_rank(5) (the 6th rank from the mover's perspective, absolute index). Debug-only, so release movegen codegen is unchanged.
- FEN: parse_ep_square now takes the parsed turn. It parses the square first, then rejects (FenError::EnPassantSquareInvalid) any square whose rank != turn.relative_rank(5) — 6th rank for White to move, 3rd for Black. The unreachable-target canonicalization (canonicalize_ep_square) still runs afterwards for legal-but-uncapturable targets; this new check only rejects the structurally-impossible rank/side mismatch.
- moves_bb PieceType::None arm: replaced bare panic!()+TODO with unreachable!("...") plus a comment noting no PieceTrait maps to None, so the arm is statically dead and exists only for match exhaustiveness. Release behaviour unchanged (arm was already unreachable dead code).
- Surveyed all FEN literals in the workspace: none are white-to-move-with-3rd-rank or black-to-move-with-6th-rank, so no existing test/perft FEN is invalidated.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 20:14
---
Implementation handoff
Branch: task-47-assert-ep-piece-invariants
Worktree: /Users/seabo/seaborg-worktrees/task-47-assert-ep-piece-invariants
Base: aa915d85d32d03d829d0636c6af3e71b40a6632f
Implementation target: 9f07b54ba6d9f35e1512ccd718ade766c5de9c28
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (core 264 passed / 2 ignored; new fen tests included; all other suites green)
Note on AC #5: the added hot-path assertions are debug_assert_eq! (compiled out in release), and the moves_bb None arm was already statically-dead, so release movegen/perft codegen is unchanged by construction; no benchmark regression is possible from these edits.
Known failures: none
---

author: @claude
created: 2026-07-19 20:20
---
Review attempt: 1
Reviewed branch: task-47-assert-ep-piece-invariants
Reviewed implementation: 9f07b54ba6d9f35e1512ccd718ade766c5de9c28
Base: aa915d85d32d03d829d0636c6af3e71b40a6632f
Verdict: approved

All six acceptance criteria proven against the immutable base..target diff (only core/src/movegen.rs and core/src/position/fen.rs changed):
- AC#1: debug_assert_eq! of the ep-rank invariant added to both the generate and has-any-move paths.
- AC#2: parse_ep_square now takes turn and returns FenError::EnPassantSquareInvalid on a rank/side mismatch rather than panicking.
- AC#3: rejects_en_passant_square_inconsistent_with_side_to_move (white 3rd-rank + black 6th-rank) and accepts_en_passant_square_consistent_with_side_to_move (valid d6) both pass.
- AC#4: PieceType::None arm is unreachable!() with a comment stating why it is statically unreachable.
- AC#5: both added asserts are debug_assert_eq! and no profile enables debug-assertions in release/bench; the None arm was already dead in release. Release codegen is unchanged by construction, so no benchmark regression is possible.
- AC#6: no TODOs remain at any of the four sites.

Verification (on 9f07b54):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (fresh CARGO_TARGET_DIR)
- cargo test --workspace: green (core 264 passed / 2 ignored)

No implementation file changed between the approved target 9f07b54 and the approval commit.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed four unchecked invariants at their boundaries. Movegen's generate and has-any-move ep paths now debug_assert_eq! that ep_square is on the 6th rank from the mover's perspective (PL::player().relative_rank(5)); fen::parse_ep_square takes the side to move and returns FenError::EnPassantSquareInvalid for a rank/side mismatch (White->6th, Black->3rd) instead of leaving the check open; moves_bb's PieceType::None arm is now unreachable!() with a comment explaining no PieceTrait maps to None; all four TODOs are removed. Verified on target 9f07b54: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings clean (fresh CARGO_TARGET_DIR); cargo test --workspace green (core 264 passed/2 ignored, including new rejects_/accepts_en_passant_square tests). AC#5 is provable rather than measured: both new asserts are debug_assert_eq! and no profile enables debug-assertions in release/bench, and the None arm was already statically-dead, so release movegen/perft codegen is unchanged and no regression is possible.
<!-- SECTION:FINAL_SUMMARY:END -->
