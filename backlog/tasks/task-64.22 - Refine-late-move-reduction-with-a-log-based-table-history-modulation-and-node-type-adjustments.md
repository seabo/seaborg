---
id: TASK-64.22
title: >-
  Refine late move reduction with a log-based table, history modulation, and
  node-type adjustments
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-21 21:22'
updated_date: '2026-07-22 00:29'
labels:
  - search
  - strength
dependencies:
  - TASK-51
parent_task_id: TASK-64
priority: medium
ordinal: 130000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The late move reduction landed in TASK-51 is correct but coarse: `lmr_reduction` (engine/src/search.rs) is a hand-tuned step function returning 1 ply, or 2 only when `depth >= 8 && move_count >= 8`. It ignores the move-ordering signals the engine already computes, so late moves deep in a long quiet list are under-reduced and the reduction never scales with how promising a move actually is.

The infrastructure needed to modulate the reduction is already merged: main history (TASK-64.2), counter-move and continuation history (TASK-64.10), and the improving signal (TASK-64.12). This task spends that signal on the reduction amount to widen effective search depth without strength loss.

Scope: (1) replace the step function with a precomputed reduction table indexed by remaining depth and move count, growing roughly like a log(depth)*log(move_count) curve; (2) modulate the base reduction by the moving side accumulated quiet history (main + continuation) so well-scored quiets reduce less and poorly-scored quiets reduce more; (3) reduce one extra ply when the side to move is not improving; (4) reduce less on PV nodes and for killer/counter moves so the ordering prefix keeps its depth. Preserve every existing safety property from TASK-51: the first move and moves that give check or receive an extension are never reduced, the reduced scout always keeps at least one ply, and any reduced scout that beats alpha is re-searched at full depth before it can enter the PV.

Out of scope (defer): cut-node-specific reduction schemes and TT-capture-driven adjustments, which pair with singular extensions (TASK-64.13) and are kept separate to preserve clean strength attribution.

Measurement discipline: each refinement must be individually gated so its strength contribution can be isolated, and net strength must be confirmed by a round-robin base-vs-target match at a real time control (not a fixed node budget, which inflates search-pruning changes), with the result and attribution recorded in BENCHMARKS.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The coarse step-function reduction is replaced by a precomputed table indexed by remaining depth and move count that grows monotonically in both
- [x] #2 The applied reduction is decreased for quiet moves with strong accumulated history (main and continuation) and increased for weak history
- [x] #3 A non-improving side to move receives an additional ply of reduction; an improving one does not
- [x] #4 PV nodes and killer/counter moves receive less reduction than a plain late quiet move at the same depth and move count
- [x] #5 All TASK-51 safety properties still hold: move one, checking moves, and extended moves are never reduced; the reduced scout keeps at least one ply; and every reduced scout that raises alpha is re-searched at full depth before populating the PV, verified by the existing TASK-36 PV-legality and TASK-51 soundness tests
- [x] #6 Each refinement is independently toggleable so its individual effect can be measured
- [x] #7 Net strength is confirmed by a round-robin base-vs-target match at a fixed time control showing no regression, with results and attribution recorded in BENCHMARKS.md
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a process-global log-based reduction table (LazyLock) indexed by [remaining depth][move count], milliplies fixed-point (x1024), growing ~ LMR_BASE + ln(depth)*ln(move_count)/LMR_DIVISOR; monotonic in both. Expose base() for tests.
2. Rework lmr_reduction into a pure free function taking (depth, move_count, pv, improving, favoured, quiet_history) and returning plies, reading the table plus per-refinement compile-time toggles.
3. Refinements, each behind a pub const bool so its strength contribution is isolable (release ablation, matching the FOLD_COUNTER_INTO_QUIETS/KILLER_SLOTS pattern):
   - LMR_LOG_TABLE: log table base vs old step function.
   - LMR_HISTORY_MODULATION: subtract a clamped term proportional to combined main+continuation quiet history (strong quiets reduce less, weak reduce more).
   - LMR_IMPROVING_MODULATION: +1 ply when the side to move is not improving.
   - LMR_FAVOURED_MODULATION: subtract at PV nodes and for killer/counter moves.
4. Call site (engine/src/search.rs move loop): compute quiet_history and favoured (killer_slot or counter) before make_move; pass improving and Node::pv(); keep every TASK-51 guard (move 1 / checking / extended never reduced; clamp to [0, new_depth-1] so the scout keeps >=1 ply; alpha-raising reduced scout re-searched at full depth).
5. Unit tests: table monotonicity (AC1); history direction (AC2); improving +ply (AC3); pv/favoured less reduction (AC4); reuse existing TASK-36 PV-legality and TASK-51 soundness/tree-reduction tests (AC5); toggle coverage (AC6).
6. Required checks (fmt/clippy/test), then round-robin base-vs-target fastchess match at a real TC; record Elo + per-refinement attribution in BENCHMARKS.md (AC7).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the log-based LMR table plus history/improving/node-type modulation in engine/src/search.rs.

Design:
- LmrTable: process-global (LazyLock) 256x256 milliplies table, base(depth, move_count) = round(1024 * (LMR_BASE + ln(depth)*ln(move_count)/LMR_DIVISOR)); monotonic non-decreasing in both. LMR_BASE=0.5, LMR_DIVISOR=2.0 keep shallow near-forcing lines at ~1 ply (old step-function behaviour) while deep late moves reduce 3-4+ plies.
- lmr_reduction() reworked into a pure free function (depth, move_count, pv, improving, favoured, quiet_history) -> plies, accumulating in milliplies and dividing to whole plies once, never negative.
- Modulations, each behind a pub const toggle for release ablation: LMR_HISTORY_MODULATION (subtract clamped quiet_history/40, cap +/-2 ply), LMR_IMPROVING_MODULATION (+1 ply when not improving), LMR_FAVOURED_MODULATION (-1 ply on PV, -1 ply for killer/counter). LMR_LOG_TABLE selects table vs old step function.
- Call site samples quiet_history_score() and favoured (killer_slot or is_counter_move()) before make_move (mover still on origin, side unflipped); passes improving and Node::pv(). All TASK-51 guards unchanged; clamp to [0, new_depth-1].

Tests: lmr_base_table_grows_monotonically_in_depth_and_move_count (AC1); lmr_eases_with_strong_history_and_deepens_with_weak (AC2); lmr_non_improving_reduces_one_extra_ply (AC3); lmr_favours_pv_nodes_and_ordering_refutations (AC4); lmr_never_returns_a_negative_reduction; existing TASK-36 PV-legality and TASK-51 soundness/tree-reduction tests still pass (AC5); four pub const toggles (AC6).

child_mate_windows_preserve_distance_parity: the more aggressive reduction defers this fixed-depth mate-in-4 by one iteration (depth 6 -> 7, deterministic single-thread; verified old step function finds it at 6). The test's subject is mate-score parity plumbing, exercised identically by the depth-7 aspiration re-search; updated the harness depth 6->7 and its docstring.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-21 22:23
---
Revisit flag from TASK-64.21 (main-search SEE pruning, closed as a negative result): a properly-gated Stockfish-style main-search SEE prune measured NO gain at a fair time control (-17 +/- 31 Elo; the +137 at nodes=100000 was a node-budget artifact). Diagnostics showed the prune is well-targeted (flagged quiets cut off at 1.35% vs a 4.03% baseline) but LOW-LEVERAGE precisely because the current lmr_reduction this task replaces is nearly a no-op (lmr_depth ~ raw depth), so the lmrDepth-scaled prune is inert. Once this LMR refinement lands (aggressive, history/depth-scaled reductions), main-search SEE pruning becomes worth re-measuring — its leverage in Stockfish comes from exactly that kind of LMR plus continuation-history pre-filtering. Suggestion for whoever picks up this task: after it merges, file a fresh ticket to re-attempt main-search SEE pruning; the prior implementation + mechanism diagnostics live in branch task-64.21-main-search-see-pruning (target 2353acb).
---

author: @codex
created: 2026-07-22 00:13
---
Implementation handoff
Branch: task-64.22-lmr-refinement
Worktree: /Users/seabo/seaborg-worktrees/task-64.22-lmr-refinement
Base: c4a655825bb4306557458fa82e4730cd0c5b8b12
Implementation target: dc943ffd9d76497ecc9c8b76cada9df0395927b8
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (all suites; engine lib 404 passed, 2 ignored)
- Strength (AC7): round-robin base-vs-target SPRT tc=8+0.08, elo0=-5 elo1=0 alpha=beta=0.05 -> PASS at 670 games, +84.6 +/- 20.1 Elo (W-D-L 280-270-120, 0 crashes/forfeits). Baseline git:708486f (engine code identical to merge-base c4a6558) vs candidate git:e8684e9; recorded in BENCHMARKS.md. Report archived at /tmp/lmr-strength/report (report.json, games.pgn, runner.log).
Known failures: none
---

author: @codex
created: 2026-07-22 00:29
---
Review attempt: 1
Reviewed branch: task-64.22-lmr-refinement
Reviewed implementation: dc943ffd9d76497ecc9c8b76cada9df0395927b8
Verdict: approved

Full base(c4a6558)->target(dc943ff) diff reviewed: engine/src/search.rs, BENCHMARKS.md, task file only; no accidental or unrelated changes. Post-target commit e4facd4 touches only the task file (handoff metadata). Tested candidate e8684e9 and target dc943ff have identical engine code; baseline 708486f has engine code identical to merge-base c4a6558.

Acceptance criteria (all proven by objective evidence):
- AC1 log table replaces step function, monotonic in depth and move count -> lmr_base_table_grows_monotonically_in_depth_and_move_count.
- AC2 history eases/deepens the cut -> lmr_eases_with_strong_history_and_deepens_with_weak.
- AC3 non-improving side takes exactly one extra ply -> lmr_non_improving_reduces_one_extra_ply.
- AC4 PV nodes and killer/counter reduced less -> lmr_favours_pv_nodes_and_ordering_refutations.
- AC5 TASK-51 safety preserved (call-site guards intact: quiet-only, extension==0, !in_check, move-one exempt, clamp to new_depth-1, alpha-raising reduced scout re-searched) -> late_move_reduction_does_not_change_sound_search_results, reported_principal_variations_are_legal, a_node_searched_past_the_nominal_horizon_still_reports_a_legal_pv, child_mate_windows_preserve_distance_parity (legitimately re-anchored 6->7 as the aggressive reduction defers a fixed-depth mate-in-4 by one iteration; the test's subject, mate-score parity, is unchanged and still asserts Score::mate(7)).
- AC6 four independent compile-time toggles -> LMR_LOG_TABLE, LMR_HISTORY_MODULATION, LMR_IMPROVING_MODULATION, LMR_FAVOURED_MODULATION.
- AC7 net strength at a real time control (not a node budget) -> BENCHMARKS.md: SPRT PASS, +84.6 +/- 20.1 Elo, 670 games, tc=8+0.08.

Verification (run on the implementation target):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean CARGO_TARGET_DIR, no warnings; no new #[allow] added)
- cargo test --workspace: pass (engine lib 406 tests, all suites green)
- Hot-path benches not required: the change is in the search path, not movegen, and its per-node cost is already charged by the timed SPRT match.

Approved. Code target for merge: dc943ffd9d76497ecc9c8b76cada9df0395927b8.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaces the coarse two-step late-move reduction with a process-global log-shaped table (base = round(1024*(LMR_BASE + ln(depth)*ln(move_count)/LMR_DIVISOR)), monotonic in both axes) plus three modulations accumulated in milliplies: combined main+continuation quiet history (strong quiets reduce less, weak reduce more), a +1 ply penalty when the side to move is not improving, and less reduction on PV nodes and for killer/counter moves. All TASK-51 call-site guards are preserved (quiet-only, unextended, non-checking, move-one exempt, clamp to new_depth-1 so the scout keeps >=1 ply, alpha-raising reduced scout re-searched at full depth). Verified on target dc943ff: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -D warnings pass (clean CARGO_TARGET_DIR); cargo test --workspace pass including new LMR tests (AC1-AC4, negative-reduction), TASK-51 soundness (late_move_reduction_does_not_change_sound_search_results), and TASK-36 PV-legality (reported_principal_variations_are_legal). Strength (AC7) recorded in BENCHMARKS.md: SPRT round-robin at real tc=8+0.08, +84.6 +/- 20.1 Elo over 670 games, baseline 708486f (engine code identical to merge-base c4a6558) vs candidate e8684e9 (engine code identical to target).
<!-- SECTION:FINAL_SUMMARY:END -->
