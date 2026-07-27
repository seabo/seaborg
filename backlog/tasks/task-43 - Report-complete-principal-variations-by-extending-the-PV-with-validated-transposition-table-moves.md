---
id: TASK-43
title: >-
  Report complete principal variations by extending the PV with validated
  transposition-table moves
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-18 13:59'
updated_date: '2026-07-27 13:22'
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
- [x] #1 A search that reports a mate score reports the full mating line: the PV length in plies equals the mate distance whenever that line is recoverable from the triangular table plus the transposition table
- [x] #2 Every move in every reported PV is still legal in the position reached after playing the preceding PV moves, on mate-scored, beta-cutoff and ordinary lines; the existing reported_principal_variations_are_legal regression test still passes unmodified
- [x] #3 PV extension terminates safely and never emits a wrong or unbounded line: a transposition-table miss, a stale or illegal TT move, a repetition, and a length cap each stop the extension, and a test covers each of these stop conditions
- [x] #4 FastChess (or cutechess) seaborg self-play at fixed depth produces zero "Illegal PV move" warnings across a multi-game match
- [x] #5 The engine selected/played best move is unchanged and search node counts are identical to the pre-change build for the same position and depth, proving PV extension happens only at reporting time and does not perturb the search
- [x] #6 A test asserts reported PV length, not just legality, for at least one known forced-mate position and one non-mate position searched to a fixed depth
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

Rework for review attempt 1 (changes requested).

Resolved REV-1-01 [P1] (fifty-move-rule PV stop): extend_pv() in engine/src/search.rs now stops the reported-PV walk on pos.fifty_move_rule_reached() as well as pos.in_threefold(), checked before extending so the move that reaches the draw is kept and nothing after it is reported. New unit test pv_extension_stops_on_a_fifty_move_rule_draw (engine/src/search/tests.rs) sets a root one ply below the fifty-move limit, offers a further TT move from the drawn position, and asserts the walk keeps only the move that reaches the draw. Re-ran the reviewer's fixed-depth self-play (release seaborg vs seaborg, depth 9, 48 games, run in /tmp so no worktree artifact): 0 'PV continues after fifty-move rule' warnings (reviewer measured 924 on the prior target), 0 'PV continues after threefold', 0 'Illegal PV move'; result 6W/6L/36D, Elo 0.00 as expected for identical-engine self-play.

Resolved REV-1-02 [P2] (stray config.json): removed config.json from the branch (git rm) and added /config.json to the repo-root .gitignore so a fastchess run from the root neither commits the results file nor dirties the worktree. Worktree left clean.
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

author: @claude
created: 2026-07-27 12:41
---
Review attempt: 1
Reviewed branch: task-43-hybrid-pv-tt-extension
Reviewed implementation: a9ed22d8ba94939978358213695a0fc64109a88e
Verdict: changes_requested

REV-1-01 [P1] PV extension does not stop at fifty-move-rule draws
Location: engine/src/search.rs, extend_pv() — the draw stop only tests pos.in_threefold().
Impact: The TT walk keeps extending the reported PV through positions already drawn by the fifty-move rule, so fastchess emits "PV continues after fifty-move rule" warnings. This is the exact class of defect the implementation already fixes for threefold repetition (the in_threefold stop plus pv_extension_stops_on_a_threefold_against_game_history), left unhandled for the other reversible-draw rule. It is a regression introduced by this change and undercuts the task's stated goal of making PV reporting more legible.
Reproduction: Build release on base a56acdd and on target a9ed22d; for each run
  fastchess -engine cmd=<bin> name=A -engine cmd=<bin> name=B -each proto=uci depth=9 \
    -openings file=tools/strength/openings-v1.epd format=epd order=sequential -rounds 24 -repeat -concurrency 6
  then grep the log for "PV continues after fifty-move rule".
  base a56acdd: 0 warnings across 48 games. target a9ed22d: 924 warnings across 48 games. Both builds emit 0 "Illegal PV move" and 0 "PV continues after threefold" warnings.
Expected: The extension stops at a fifty-move-rule draw exactly as it stops at a threefold — keep the move that first reaches the draw, report nothing after it. chess::Position::fifty_move_rule_reached() already exists; a symmetric check beside the in_threefold() stop resolves it. Add a unit test mirroring the threefold stop test, and re-run fixed-depth self-play to confirm the warnings are gone.
Note on AC #4: AC #4 as literally worded (zero "Illegal PV move" warnings) is met, but the implementer's own verification bar and notes extended to "PV continues past threefold"; the fifty-move analog is the same reporting defect and is a measured regression against base.

REV-1-02 [P2] Unrelated fastchess artifact committed to the branch and left dirty in the worktree
Location: config.json (repo root), introduced in handoff commit fee137a; also present as an uncommitted modification in the worktree.
Impact: config.json is a fastchess config/results file. It is absent at base a56acdd and at the code target a9ed22d, is untracked on master, and is not gitignored. It was committed into the handoff commit (152 lines) and would land on primary at merge, and it leaves the worktree dirty on every self-play run. This violates scope discipline, the "commits after the implementation target contain only handoff metadata" rule, and the clean-worktree requirement.
Reproduction: git cat-file -t a9ed22d:config.json (absent) vs git show --stat fee137a (adds config.json); git status shows config.json modified in the worktree.
Expected: Drop config.json from the task branch (it must not be in the handoff commit) and add the fastchess artifact to .gitignore so it neither gets committed nor dirties the worktree. Leave the worktree clean.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (confirmed with a clean CARGO_TARGET_DIR)
- cargo test --workspace: pass (all suites; new PV tests and reported_principal_variations_are_legal green)
- fixed-depth self-play A/B (base a56acdd vs target a9ed22d, depth 9, 48 games): base 0 / target 924 "PV continues after fifty-move rule" warnings; 0 "Illegal PV move" on both
---

author: @claude
created: 2026-07-27 12:52
---
Implementation handoff (rework, attempt 1)
Branch: task-43-hybrid-pv-tt-extension
Worktree: /Users/seabo/seaborg-worktrees/task-43-hybrid-pv-tt-extension
Base: a56acdde4c2c5d83c2d1e02c5d9be1993a74e9e3
Implementation target: 11ea6f5
Resolved findings: REV-1-01, REV-1-02
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (all suites, 0 failed; new pv_extension_stops_on_a_fifty_move_rule_draw green, reported_principal_variations_are_legal unchanged and green)
- fixed-depth self-play (release seaborg vs seaborg, depth 9, 48 games, 36 draws): 0 'PV continues after fifty-move rule', 0 'PV continues after threefold', 0 'Illegal PV move' warnings
Known failures: none
---

author: @claude
created: 2026-07-27 13:22
---
Review attempt: 2
Reviewed branch: task-43-hybrid-pv-tt-extension
Reviewed implementation: 11ea6f5 (immutable; base a56acdd; branch tip fa3cddb adds task metadata only)
Verdict: approved

Both attempt-1 findings resolved:
- REV-1-01 [P1] fifty-move PV stop: extend_pv() now stops on pos.fifty_move_rule_reached() beside pos.in_threefold(), checked before extending so the move reaching the draw is kept and nothing after it is reported. Independently re-ran the reviewer's self-play (fastchess seaborg vs seaborg, depth 9, 48 games incl. 3-fold and fifty-move draws): 0 'PV continues after fifty-move rule' (was 924 on the prior target), 0 'PV continues after threefold', 0 'Illegal PV move'. New unit test pv_extension_stops_on_a_fifty_move_rule_draw covers it.
- REV-1-02 [P2] stray config.json: removed from the branch and /config.json added to .gitignore; net base..target diff contains no config.json and the worktree is clean.

Acceptance criteria (all proven):
- AC1: a_resolved_mate_reports_the_full_mating_line asserts reported PV length == plies-to-mate.
- AC2: reported_principal_variations_are_legal byte-unchanged (only additions after it) and green over extended PVs.
- AC3: six stop-condition tests — TT miss, stale/illegal move, in-line cycle, threefold-against-history, fifty-move draw, length cap.
- AC4: independently verified — fastchess depth 9, 48 games, 0 illegal-PV / threefold-continuation / fifty-move-continuation warnings.
- AC5: pv_extension_preserves_the_exact_prefix_and_visits_no_nodes proves node count unchanged; reported_pv() runs once per iteration in emit_progress, outside the node loop, cloning+probing only.
- AC6: mate + non-mate length assertions plus the deterministic cap test.

Verification commands (on target 11ea6f5):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (fresh compile, no warnings)
- cargo test --workspace: pass (0 failed)
- fastchess -engine cmd=seaborg name=A -engine cmd=seaborg name=B -each proto=uci depth=9 -openings file=tools/strength/openings-v1.epd format=epd order=sequential -rounds 24 -repeat -concurrency 6: 0 PV warnings across 48 games

No new #[allow] introduced; comments are self-contained and explain the reasoning (stale-sibling splice, reversible-draw rules), not just task references. Code target 11ea6f5 remains the immutable implementation; this approval commit adds task metadata only.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reporting-only hybrid PV: reported_pv()/extend_pv() in engine/src/search.rs take the triangular table's exact line as a trusted prefix and extend it by walking TT best moves on a clone of the root, validating each ply (pseudolegal valid_move + enemy_in_check) and stopping on TT miss, stale/illegal move, in-line cycle, threefold or fifty-move draw against full history, or the MAX_PLY cap. Called once per iteration in emit_progress (line 1807), outside the node loop, so the played move and node counts are unchanged. Verified on immutable target 11ea6f5: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings clean (fresh compile); cargo test --workspace 0 failed; new PV tests cover full-mate length (AC1/6), non-mate length (AC6), and all six stop conditions (AC3), reported_principal_variations_are_legal unchanged and green (AC2), pv_extension_preserves_the_exact_prefix_and_visits_no_nodes proves the reporting-only invariant (AC5). AC4 independently verified: fastchess seaborg self-play depth 9, 48 games (incl. 3-fold and fifty-move draws) — 0 'Illegal PV move', 0 'PV continues after threefold', 0 'PV continues after fifty-move rule' warnings.
<!-- SECTION:FINAL_SUMMARY:END -->
