---
id: TASK-43
title: >-
  Report complete principal variations by extending the PV with validated
  transposition-table moves
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-18 13:59'
updated_date: '2026-07-27 10:41'
labels:
  - engine
  - search
  - uci
dependencies:
  - TASK-64.1
priority: low
type: enhancement
ordinal: 44000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-36 made every reported "info ... pv ..." line legal by publishing only plies that came from an exact PV-node alpha raise, with the triangular PVTable row cleared on entry to every node. That is correct, but deliberately conservative: a fail-high node has no exact continuation to publish, and the mating move at the end of a forced line usually arrives as a fail-high. The result is that mate-scored lines are reported truncated. Measured on the TASK-36 diff, a position scored "mate 3" reports "pv c7c6 a6a5" where the informative line is five plies, and all PV changes introduced by TASK-36 were on mate-scored lines (non-mate PVs were byte-identical to the previous behaviour).

This is a reporting and diagnostics defect, not a strength defect. It affects what a UCI GUI displays and how legible the search is when debugging; it does not change which move the engine plays.

The conventional remedy is a hybrid PV: keep the triangular table as the trusted exact prefix, then extend past its last ply by walking the transposition table, playing each TT move only after confirming it is legal in the position reached, and stopping on a hash miss, an illegal or stale TT move, a repetition, or a sensible length cap. This recovers full mate lines and depth-length PVs without reintroducing the stale-sibling splice that TASK-36 fixed, because every extended ply is validated against a real position rather than copied from a table row.

The verification harness already exists and should be reused rather than rebuilt: engine/src/search.rs has reported_principal_variations_are_legal, which replays every emitted PV from the root and asserts legality, and the FastChess self-play A/B at fixed depth is the end-to-end check. Relevant code: engine/src/pv_table.rs, engine/src/search.rs (emit_progress), engine/src/tt.rs, engine/src/info.rs. Background: TASK-36 and backlog doc-2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A search that reports a mate score reports the full mating line: the PV length in plies equals the mate distance whenever that line is recoverable from the triangular table plus the transposition table
- [ ] #2 Every move in every reported PV is still legal in the position reached after playing the preceding PV moves, on mate-scored, beta-cutoff and ordinary lines; the existing reported_principal_variations_are_legal regression test still passes unmodified
- [ ] #3 PV extension terminates safely and never emits a wrong or unbounded line: a transposition-table miss, a stale or illegal TT move, a repetition, and a length cap each stop the extension, and a test covers each of these stop conditions
- [ ] #4 FastChess (or cutechess) seaborg self-play at fixed depth produces zero "Illegal PV move" warnings across a multi-game match
- [ ] #5 The engine selected/played best move is unchanged and search node counts are identical to the pre-change build for the same position and depth, proving PV extension happens only at reporting time and does not perturb the search
- [ ] #6 A test asserts reported PV length, not just legality, for at least one known forced-mate position and one non-mate position searched to a fixed depth
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a reporting-only hybrid PV assembler in engine/src/search.rs: keep the triangular PVTable line as the trusted exact prefix, then extend past its last ply by walking the transposition table on a clone of the root position.
2. Validate each extended ply against the real position reached (pseudolegal via valid_move, then reject moves leaving the mover's king in check) and stop on: TT miss, move-less entry, stale/illegal move, a repeated position (cycle), or a MAX_PLY length cap.
3. Wire emit_progress to the assembler; the walk only probes the TT and mutates a local clone, so played move and node counts are unchanged.
4. Tests: mate-line length == plies-to-mate (AC1/6), non-mate length assertion (AC6), each stop condition (AC3), and confirm reported_principal_variations_are_legal still passes unmodified (AC2).
5. Run required checks; verify no illegal-PV warnings in fixed-depth self-play (AC4).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a reporting-only hybrid PV in engine/src/search.rs. reported_pv() takes the triangular PV table's exact line as a trusted prefix and extend_pv() walks the transposition table on a clone of the root position, appending each stored best move after validating it against the real position reached. Stop conditions: TT miss, move-less entry, stale/illegal move (pseudolegal check plus a king-left-in-check check), an in-line cycle (per-line seen set), a draw by repetition against the full game history (Position::in_threefold, which the per-line set cannot detect), and a MAX_PLY length cap. The walk only probes the TT and mutates the clone, so the played move and node counts are unchanged.

Discovered during fixed-depth self-play that TT extension alone (in-line cycle detection only) produces a fastchess 'PV continues after threefold repetition' warning: the walk can repeat a position from earlier in the game that is not on the reported line. Added the in_threefold draw stop to fix it; a unit test reproduces the exact case and fails without the stop.

Tests (engine/src/search/tests.rs):
- a_resolved_mate_reports_the_full_mating_line: reported mate PV length equals plies-to-mate.
- a_non_mate_search_reports_a_multi_ply_line: non-mate reported line length floor.
- pv_extension_stops_on_a_transposition_table_miss / _a_stale_or_illegal_tt_move / _a_repetition / _a_threefold_against_game_history / _at_the_length_cap: one per stop condition.
- pv_extension_preserves_the_exact_prefix_and_visits_no_nodes: reporting-only invariant (prefix preserved, node count unchanged).
- reported_principal_variations_are_legal: unchanged and still passing, now over extended PVs.

AC #4 evidence: fastchess self-play, seaborg vs seaborg, depth 8 fixed, 64 games (16 ending in 3-fold-repetition draws), concurrency 4: zero 'Illegal PV move' warnings and zero 'PV continues after threefold' warnings.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-27 10:41
---
Implementation handoff
Branch: task-43-hybrid-pv-tt-extension
Worktree: /Users/seabo/seaborg-worktrees/task-43-hybrid-pv-tt-extension
Base: a56acdde4c2c5d83c2d1e02c5d9be1993a74e9e3
Implementation target: a9ed22d8ba94939978358213695a0fc64109a88e
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (all binaries, no failures)
- fastchess self-play depth=8, 64 games, 16 three-fold draws: 0 'Illegal PV move', 0 'PV continues after threefold' warnings
Known failures: none
---
<!-- COMMENTS:END -->
