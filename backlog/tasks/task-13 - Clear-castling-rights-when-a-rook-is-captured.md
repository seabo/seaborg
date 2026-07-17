---
id: TASK-13
title: Clear castling rights when a rook is captured
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 22:56'
labels:
  - core
  - movegen
  - rules
dependencies: []
references:
  - core/src/position/mod.rs
  - core/src/position/castling.rs
  - core/src/movegen.rs
modified_files:
  - core/src/position/mod.rs
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
- [x] #1 Capturing a rook on a1, h1, a8, or h8 clears the corresponding castling right
- [x] #2 Move generation requires the correctly colored king and rook on their castling origin squares
- [x] #3 Make and unmake restore castling rights and Zobrist keys exactly across rook captures
- [x] #4 Tests cover all four rook-capture squares and stale-right FEN inputs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Update castling-right transitions during make_move to account for both the moving origin and a captured rook origin square, preserving incremental Zobrist updates and undo snapshots.
2. Harden castling generation so the side-to-move king and same-colored rook must occupy their exact origin squares before a castle is emitted.
3. Add focused position and move-generation regression tests for captures on a1, h1, a8, and h8, exact make/unmake state and Zobrist restoration, and stale-right FENs with missing or wrong-colored origin pieces.
4. Run focused tests, cargo fmt --check, and cargo test --workspace; commit implementation and prepare the immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented capture-destination castling-right invalidation for captured rooks, using the existing incremental Zobrist castling-right transition. Hardened castling generation to require the correctly colored king on e1/e8 and rook on a/h origin. Added four-square capture make/unmake regressions and stale-right FEN move-generation coverage for missing, displaced, and wrong-colored pieces.

Verification passed: cargo test -p core rook_captures_clear_castling_rights_and_unmake_restores_state; cargo test -p core stale_castling_rights_require_correct_origin_pieces; cargo fmt --check; cargo test --workspace.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 21:43
---
Implementation handoff
Branch: task-13-rook-capture-castling
Worktree: /Users/seabo/seaborg-worktrees/task-13-rook-capture-castling
Base: 2c3a91b42c8810ca1897c4fc7675470aa4245ac0
Implementation target: 31ad717e04bf7b101d75d3ecbdcb8983aa33ca4b
Resolved findings: none
Verification:
- cargo test -p core rook_captures_clear_castling_rights_and_unmake_restores_state: passed
- cargo test -p core stale_castling_rights_require_correct_origin_pieces: passed
- cargo fmt --check: passed
- cargo test --workspace: passed (87 tests passed, 1 ignored across unit/integration suites; doc tests passed)
Known failures: none
---

author: @codex
created: 2026-07-17 22:56
---
Review attempt: 1
Reviewed branch: task-13-rook-capture-castling
Reviewed implementation: 31ad717e04bf7b101d75d3ecbdcb8983aa33ca4b
Verdict: approved

All acceptance criteria proven by objective evidence:
- AC#1: make_move adds castling_rights.update(to) on rook captures; test rook_captures_clear_castling_rights_and_unmake_restores_state asserts correct residual rights for a1/h1/a8/h8.
- AC#2: castling_side requires Piece::make(player, King) on e1/e8 and Piece::make(player, Rook) on the castling origin; test stale_castling_rights_require_correct_origin_pieces covers missing rook, wrong-colored rook, and displaced king. Fixes prior bug where an opposite-colored rook satisfied the type-only check.
- AC#3: make/unmake round-trip asserts zobrist changes on make and full position + zobrist equality on unmake.
- AC#4: coverage present for all four capture squares and stale-right FEN inputs.

Scope: only core/src/movegen.rs and core/src/position/mod.rs changed (plus task file). Post-target commit is metadata-only.

Verification:
- cargo fmt --check: passed
- cargo test -p core castling: passed (2)
- cargo test --workspace: passed (all suites, 1 ignored; doc tests passed)
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rook captures now clear the matching castling right and move generation requires the correctly colored king and rook on their castling origins. make_move applies castling_rights.update(to) when the captured piece is a rook (no-op for en passant/castle, and also covers promotion-captures of corner rooks); castling_side requires exact Piece equality for the king on e1/e8 and rook on the castling-origin square. Verified in worktree: cargo fmt --check (clean); cargo test -p core castling (2 passed: rook_captures_clear_castling_rights_and_unmake_restores_state, stale_castling_rights_require_correct_origin_pieces); cargo test --workspace (all suites green incl. perft and doc tests). All four acceptance criteria proven by tests over squares a1/h1/a8/h8, wrong-color/missing-rook and displaced-king FENs, and exact make/unmake zobrist restoration.
<!-- SECTION:FINAL_SUMMARY:END -->
