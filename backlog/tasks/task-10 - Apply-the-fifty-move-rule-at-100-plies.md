---
id: TASK-10
title: Apply the fifty-move rule at 100 plies
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:16'
labels:
  - search
  - rules
dependencies: []
references:
  - engine/src/search.rs
  - core/src/position/mod.rs
modified_files:
  - core/src/position/mod.rs
  - engine/src/game.rs
  - engine/src/search.rs
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
- [x] #1 A halfmove clock of 99 does not trigger the fifty-move draw condition
- [x] #2 A halfmove clock of 100 triggers the draw condition
- [x] #3 Material evaluation is not incorrectly forced to zero at 50 plies
- [x] #4 Tests cover FEN positions immediately below, at, and above the threshold
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a Position-level fifty-move threshold and predicate expressed in plies.
2. Use the shared predicate in search and game terminal detection, and scale material evaluation over the same 100-ply window.
3. Add FEN-based boundary tests for 99, 100, and 101 plies plus a regression assertion that evaluation remains nonzero at 50.
4. Run focused tests, cargo fmt --check, and cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a shared Position::FIFTY_MOVE_RULE_PLIES threshold and fifty_move_rule_reached predicate. Search and game status now use the shared 100-ply condition, and material evaluation scales across 100 plies. Added FEN boundary coverage at 99, 100, and 101 and a 50-ply material-evaluation regression test. Updated the affected pawn-race score range while preserving its expected best move.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 19:12
---
Implementation handoff\nBranch: task-10-fifty-move-rule\nWorktree: /Users/seabo/seaborg-worktrees/task-10-fifty-move-rule\nBase: 4e7c7089431de8122541bc430ff200beb954f2e1\nImplementation target: d19aaec7d53af14ad7277ed017d872b83c5f5e48\nResolved findings: none\nVerification:\n- cargo test -p engine fifty_move_rule_uses_halfmove_boundary: passed\n- cargo test -p engine material_evaluation_scales_over_one_hundred_halfmoves: passed\n- cargo fmt --check: passed\n- cargo test --workspace: passed (56 passed, 1 ignored)\nKnown failures: none
---

author: @codex
created: 2026-07-17 19:12
---
Implementation handoff (formatted; supersedes comment #1)
Branch: task-10-fifty-move-rule
Worktree: /Users/seabo/seaborg-worktrees/task-10-fifty-move-rule
Base: 4e7c7089431de8122541bc430ff200beb954f2e1
Implementation target: d19aaec7d53af14ad7277ed017d872b83c5f5e48
Resolved findings: none
Verification:
- cargo test -p engine fifty_move_rule_uses_halfmove_boundary: passed
- cargo test -p engine material_evaluation_scales_over_one_hundred_halfmoves: passed
- cargo fmt --check: passed
- cargo test --workspace: passed (56 passed, 1 ignored)
Known failures: none
---

author: @codex
created: 2026-07-17 19:16
---
Review attempt: 1
Reviewed branch: task-10-fifty-move-rule
Reviewed implementation: d19aaec7d53af14ad7277ed017d872b83c5f5e48
Verdict: approved

Verification:
- cargo test -p engine fifty_move_rule_uses_halfmove_boundary: passed
- cargo test -p engine material_evaluation_scales_over_one_hundred_halfmoves: passed
- cargo fmt --check: passed
- cargo test --workspace: passed (61 passed, 1 ignored)

Acceptance criteria verified at halfmove clocks 99, 100, and 101; material evaluation remains nonzero at 50.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Applied a shared 100-halfmove threshold to search and game draw detection and aligned material scaling with that window. Verified boundary behavior at 99, 100, and 101 plies, nonzero evaluation at 50, formatting, and the full workspace test suite.
<!-- SECTION:FINAL_SUMMARY:END -->
