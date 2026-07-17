---
id: TASK-10
title: Apply the fifty-move rule at 100 plies
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:08'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a Position-level fifty-move threshold and predicate expressed in plies.
2. Use the shared predicate in search and game terminal detection, and scale material evaluation over the same 100-ply window.
3. Add FEN-based boundary tests for 99, 100, and 101 plies plus a regression assertion that evaluation remains nonzero at 50.
4. Run focused tests, cargo fmt --check, and cargo test --workspace.
<!-- SECTION:PLAN:END -->
