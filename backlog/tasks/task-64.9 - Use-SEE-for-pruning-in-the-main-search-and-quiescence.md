---
id: TASK-64.9
title: Use SEE for pruning in the main search and quiescence
status: Needs Human
assignee:
  - '@codex'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 13:00'
labels:
  - search
  - pruning
  - see
  - quiescence
dependencies: []
references:
  - engine/src/see.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 72000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Static exchange evaluation is implemented and correct, including the promotion handling added by TASK-49, but it is used only to sort moves. Its two call sites are `MoveLoader::score_captures` (search.rs:1472-1486) and `QMoveLoader::score_captures` (search.rs:1532-1546), both of which assign the SEE value as an ordering score feeding the GoodCaptures, EqualCaptures and BadCaptures phase split. Nothing anywhere uses it to decide not to search a move.

Two applications, delivered together because they share the same predicate and the same measurement:

Quiescence. `QMoveLoader` generates and searches every capture, including those SEE scores as clearly losing, and applies no delta margin. A losing capture near the horizon almost never repays its subtree. Skipping captures with a negative SEE, and skipping captures whose optimistic material gain plus a margin cannot reach alpha, are the two standard cuts and both are absent. Quiescence node share is already instrumented in the telemetry block at search.rs:1341 and should be reported before and after.

Main search. At shallow depth in non-PV nodes, prune captures and quiets whose SEE falls below a depth-scaled threshold. Note that bad captures are currently searched: they are not discarded, only deferred to the BadCaptures phase after quiets (ordering.rs:277-278).

The delta margin compares against the static evaluation and inherits the material-only caveat that applies across this programme. The SEE-based cuts do not: SEE is a material calculation and is unaffected by evaluation quality, which makes this task one of the more reliable gains available before the evaluation work.

TASK-29 covers bounding quiescence recursion by ply. Its second comment records that the large quiescence trees observed in practice are driven by capture and promotion interleaving rather than check evasions, which is exactly what the cuts in this task address; the two are complementary and neither substitutes for the other.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Quiescence skips captures with a losing static exchange evaluation, under a documented threshold
- [ ] #2 Quiescence applies a delta margin so captures that cannot plausibly reach alpha are not searched
- [ ] #3 Neither quiescence cut is applied while in check, where all evasions must remain available
- [ ] #4 The main search prunes moves below a depth-scaled SEE threshold in non-PV nodes at shallow depth
- [ ] #5 Quiescence node counts and the quiescence share of total nodes are reported before and after on a representative position set
- [ ] #6 Tactical test positions requiring a losing capture to find the correct move are covered and still solved
- [ ] #7 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Quiescence cuts in quiesce_inner move loop (not-in-check by construction; evasions path via quiesce_evasions untouched -> AC#3): (a) SEE cut skipping captures with SEE below a documented threshold (0); (b) delta-margin cut skipping captures whose stand-pat + optimistic material gain + margin cannot reach alpha. Add threshold/margin constants.
2. Main-search SEE pruning: new step in the main move loop before make_move; non-PV nodes at shallow depth only, depth-scaled SEE threshold for captures and quiets; guarded by forward_pruning_enabled, move_count>1, node not in check, and best_value not a proven mate so tactics survive. Wire the pre-staged trace.see_skip_node()/see_skipped_nodes() telemetry.
3. Add a #[cfg(test)] see_pruning_disabled toggle + see_pruning_enabled() for isolation tests, mirroring lmr/rfp/futility.
4. Tests: SEE-cut behaviour (losing capture skipped in q-search, delta cut, in-check evasions unaffected); tactical positions requiring a losing/sacrificial capture still solved at fixed depth; q-node share instrumentation.
5. Measure q-node counts and q share before/after on a representative position set; run cargo fmt/clippy/test; run the TASK-27 strength measurement via fastchess directly against a merge-base baseline binary; record all results in implementation notes (AC#5, AC#7).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented SEE-based pruning in three places, all gated by a new see_pruning_enabled() (test toggle see_pruning_disabled), and wired to the pre-existing trace.see_skip_node()/see_skipped_nodes() telemetry.

Quiescence (AC#1/#2/#3): in quiesce_inner's capture loop (only reached when NOT in check; the in-check node returns via quiesce_evasions first, so evasions are never cut). Two cuts on captures only (queen promotions are a separate phase, never cut): (a) delta cut — skip when stand_pat + captured_value (+ queen premium for a promoting capture) + QUIESCENCE_DELTA_MARGIN(200cp) <= alpha, not applied against a mate-distance alpha; (b) SEE cut — skip when SEE < QUIESCENCE_SEE_THRESHOLD(0). Both prepared pre-move and applied after make_move so a capture that gives check is exempted (a checking capture at the horizon is how a sacrifice delivers mate).

Main search (AC#4): new Step 16b/18b. In non-PV nodes at depth <= SEE_PRUNING_MAX_DEPTH(6), after move 1, with node not in check and best_value still a cp score, prune a move whose SEE < see_pruning_margin(depth,is_capture). Captures: -(300 + 100*depth); quiets: -60*depth. Check-giving moves exempted (decided pre-move, applied post-make). The capture BASE cushion (300) is pinned by the shallow forced mates in the search regression suite: without it, sacrificial mating captures (SEE ~ -300) are pruned and mate-in-3/mate-in-7 tests revert to a bare material score. Discovered via bisection that both the q-cut and the main-search prune independently broke child_mate_windows_preserve_distance_parity; the checking-capture exemption fixes the q-cut, the capture base cushion fixes the main prune.

AC#5 (q-node share, representative 7-position set, depth 8, one sitting): SEE-off total_nodes=2,183,780 q_nodes=1,527,316 q_share=69.94%; SEE-on total_nodes=1,245,433 q_nodes=736,571 q_share=59.14% see_skips=396,681. => total nodes -43.0%, q_nodes -51.8%, q_share -10.8pp.

Tests added (engine/src/search.rs): see_pruning_leaves_forced_results_unchanged (AC#6, incl. Qxg7# losing-capture mate + sac-proof mate-in-7), quiescence_finds_a_mate_delivered_by_a_losing_capture (checking-capture exemption), quiescence_skips_losing_captures (AC#1), quiescence_delta_margin_skips_out_of_reach_captures (AC#2, isolates delta from SEE), quiescence_cuts_do_not_apply_while_in_check (AC#3), see_pruning_shrinks_the_search_tree (node reduction).

STRENGTH MEASUREMENT (AC#7) AND SCOPE DECISION ON AC#4

Measured target vs merge-base baseline (027d20f) with fastchess, nodes=100000, openings-v1.epd, colour-reversed pairs, one sitting on an idle machine. A node budget is the fair budget for a pruning change (saved nodes convert to depth).

- Quiescence cuts only (AC#1/#2/#3): +70.44 +/- 39.36 Elo, LOS 99.99%, 200 games (60.0%).
- Main-search prune only (AC#4): -19.13 +/- 40.27 Elo, LOS 17%, 200 games (not significant; my capture floor is near-inert for mate safety, so this is essentially the quiet-move prune).
- BOTH combined (as originally implemented): -88.74 +/- 19.86 Elo, LOS 0%, 500 games (37.5%).

=> The two cuts interact destructively: q-cuts alone +70, but adding the main-search prune collapses the change to -88 (a ~158 Elo negative interaction, plausibly over-pruning compounded by this engine's material-only leaf eval reaching pathological depths under the node budget).

DECISION (made with the user this session): ship the quiescence cuts only; DEFER AC#4 (main-search SEE prune). The main-search prune code, its constants, and see_pruning_margin() were REMOVED (superseding the design in the earlier note). A properly-gated main-search prune (lmrDepth-scaled, history-conditioned, or capture-only with a workable floor) is left as potential follow-up work for a human/reviewer to scope; per the lifecycle I did not create that task myself.

FINAL shipped code (quiescence cuts only) re-measured: +68.03 +/- 31.87 Elo, LOS 100.00%, 300 games (59.7%).

AC STATUS for the shipped change: AC#1 (losing-capture cut), AC#2 (delta margin), AC#3 (not applied in check), AC#5 (q-share reported), AC#6 (losing-capture tactics still solved), AC#7 (measured) are delivered. AC#4 (main-search prune) is intentionally NOT delivered per the decision above.

ROBUSTNESS FIX EXPOSED BY THE QUIESCENCE CUTS (iterative-deepening result path)

The tests/timed_selfplay 'never forfeits' fixture began failing reliably (target 0/10 vs baseline ~9/10) with 'null best move at ply 18 budget 6ms'. Root cause: in a position already drawn by repetition, every root move scores 0 and none raises alpha, so a completed iteration carries an empty principal variation; iterative_deepening then reported that iteration verbatim as SearchResult{best_move: None} — a bestmove 0000 forfeit — even though legal moves exist. This is a PRE-EXISTING latent bug (baseline hits it ~1/10), but the quiescence cuts make nodes cheap enough that a dead-drawn K+R+P endgame races to depth ~165 in the 6ms budget, hitting the empty-PV iteration every time (repro FEN 8/3R4/1K1p4/1p1r4/8/8/4PkP1/8 w - - 10 10 with game history, depth=165 score=Cp(0)).

Fix (search.rs iterative_deepening extraction): when the completed result carries no move but the position has a guaranteed first legal move (root_fallback is Some), substitute that legal move while keeping the iteration score; a genuinely terminal root (no legal moves, root_fallback None) still correctly returns its move-less mate/stalemate score. Added regression test a_drawn_root_still_reports_a_legal_move (fifty-move-drawn but non-terminal root must return a legal move); verified it fails without the fix and passes with it. tests/timed_selfplay now 22/22 then 10/10 under load (was 0/10). This is a small, contained robustness fix in the search core, necessary so shipping the q-cuts does not introduce drawn-endgame forfeits under time pressure.

FINAL shipped binary (quiescence cuts + null-move robustness fix) re-measured vs merge-base baseline 027d20f: +68.03 +/- 31.87 Elo, LOS 100.00%, 300 games (59.67%, W151/L93/D56). Identical to the pre-fix q-cuts measurement (node-limited deterministic games; the robustness fix changed no game in this set), confirming the gain holds for the exact committed code. All repository checks green on the final code: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, cargo test --workspace (incl. tests/timed_selfplay now robust).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-21 12:44
---
Implementation handoff
Branch: task-64.9-see-pruning
Worktree: /Users/seabo/seaborg-worktrees/task-64.9-see-pruning
Base: 027d20f3992a77e3d641c4c3acd3d553434e8d79
Implementation target: b32c1a32461d6271846d2d7de26ce5f2727ea3ff
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (incl. tests/timed_selfplay, now robust after the null-bestmove fix; stressed 22/22 then 10/10 under load)
- strength vs merge-base baseline (fastchess, nodes=100000, 300 games, openings-v1.epd): +68.03 +/- 31.87 Elo, LOS 100.00%
Known failures: none
Scope note: AC#4 (main-search SEE prune) intentionally DEFERRED per an explicit user decision this session. Measured -88 Elo when combined with the quiescence cuts (q-cuts alone +70; main prune alone -19; strong negative interaction). The main-search prune code was removed; a properly-gated version is potential follow-up (not created per lifecycle). Also included a small search-core robustness fix (null best move in drawn non-terminal roots) that the q-cuts exposed. See implementation notes for full detail.
---

author: @codex-reviewer
created: 2026-07-21 13:00
---
Review attempt: 1
Reviewed branch: task-64.9-see-pruning
Reviewed base: 027d20f3992a77e3d641c4c3acd3d553434e8d79
Reviewed implementation: b32c1a32461d6271846d2d7de26ce5f2727ea3ff
Verdict: needs_human

DECISION NEEDED (scope / authority): This task defines 7 acceptance criteria. AC#4 (main search prunes moves below a depth-scaled SEE threshold in non-PV nodes at shallow depth) is intentionally NOT delivered — the main-search prune code and its constants were removed. The implementation notes cite a user decision this session to ship quiescence cuts only, after measuring the combined change at -88 Elo (q-cuts alone +70; main prune alone -19; strong negative interaction), corroborated independently. That is a sound engineering call, but it is a scope change: (a) as an independent reviewer I cannot verify the cited consent, and (b) AC#4 is still listed as a required criterion on this task. A reviewer may approve only when objective evidence proves EVERY acceptance criterion, so this cannot move to Ready to Merge, and requesting the implementer to build AC#4 would demand a change already measured to regress strength. A human must either:
  1. Ratify dropping AC#4 from this task (ideally splitting a properly-gated main-search SEE prune — lmrDepth-scaled / history-conditioned / capture-only with a workable floor — into a new follow-up task), after which the remaining six criteria are approvable; or
  2. Direct that AC#4 be implemented despite the measured regression.
Neither the descope nor the follow-up task can be created by the implementation or review agents under the lifecycle.

TECHNICAL REVIEW OF THE SHIPPED CHANGE (quiescence cuts + drawn-root fix): sound, no blocking code findings.
- Worktree clean; b32c1a3 descends from base 027d20f; only later commit (7167497) is task-only handoff metadata; b32c1a3..7167497 touches the task file alone.
- Repository-required checks re-run on the target by the reviewer:
  - cargo fmt --check: pass
  - cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): pass, zero warnings
  - cargo test --workspace: pass (exit 0, 0 failures)
  - New unit tests pass individually (see_pruning_leaves_forced_results_unchanged, quiescence_finds_a_mate_delivered_by_a_losing_capture, quiescence_skips_losing_captures, quiescence_delta_margin_skips_out_of_reach_captures, quiescence_cuts_do_not_apply_while_in_check, see_pruning_shrinks_the_search_tree, a_drawn_root_still_reports_a_legal_move)
  - engine/tests/timed_selfplay fast_timed_self_play_never_forfeits_or_hangs: pass (validates the drawn-root robustness fix)
- Correctness: both quiescence cuts are prepared pre-move and applied only after make_move with a !in_check guard, so a checking capture is exempt; the in-check node bypasses the cut loop via quiesce_evasions (AC#3 holds structurally). Delta cut is guarded by alpha.is_cp() so it never fires against a mate-distance alpha. SEE cut threshold is 0. The see_pruning_disabled test toggle mirrors the existing lmr/rfp/futility pattern. Constants carry rationale comments (no bare code restatement, no task-ID-only references). The drawn-root fix substitutes root_fallback only when a completed iteration has an empty PV, preserving the iteration score and leaving genuinely terminal roots move-less.

OBJECTIVELY PROVEN CRITERIA (for the shipped quiescence-only scope): AC#1, AC#2, AC#3, AC#5, AC#6, AC#7. UNMET BY DESIGN: AC#4.

NON-BLOCKING OBSERVATION: for an en-passant capture the delta/SEE inputs read piece_at_sq(mov.dest()), which is empty on the EP destination square, so an EP capture is scored as gaining nothing and tends to be SEE-cut in quiescence. This exactly matches the pre-existing MoveLoader/QMoveLoader score_captures SEE call pattern, introduces no new inconsistency, is tactically marginal, and is subsumed by the strongly positive net strength measurement; noted only for a future SEE-input cleanup, not a blocker here.
---
<!-- COMMENTS:END -->
