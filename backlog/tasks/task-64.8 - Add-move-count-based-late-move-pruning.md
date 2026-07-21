---
id: TASK-64.8
title: Add move-count based late move pruning
status: Done
assignee:
  - '@george'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 16:32'
labels:
  - search
  - pruning
dependencies:
  - TASK-64.2
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 71000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In non-PV nodes near the horizon, stop searching quiet moves once a depth-indexed move count has been exceeded. The move loop at search.rs:803-910 currently searches every generated move at every node regardless of how late in the ordering it appears.

Late move pruning is the cheapest of the move-loop prunings because it consults only the move counter and the depth, not the evaluation. That makes it the one pruning technique in this programme whose effectiveness does not depend on the quality of the static evaluation, and therefore the one most likely to show a clean gain before the evaluation work lands.

Its effectiveness does depend on move ordering, since discarding late moves is only safe when late genuinely means unpromising. It is gated on the history heuristic being active, because quiet moves are presently ordered by an all-zero table and there is no sense in which a quiet move is currently late.

A move counter (`move_count`, search.rs:800) already exists and is incremented per move. The staged ordering makes the phase of the current move available via `OrderedMoves::phase` (ordering.rs:540), which distinguishes quiets from captures and killers without inspecting the move.

Two adjacent items worth folding in while this code is being touched. First, the ordering's final phase is Underpromotions (ordering.rs:279), synthesised from the queen-promotion segment; most engines exclude these outside quiescence and the saving is free. Second, whether pruning should also apply to bad captures, which sit after quiets in the phase order, is worth settling and recording.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Quiet moves beyond a depth-indexed move count are not searched in non-PV nodes, and the technique is disabled in check and in PV nodes
- [x] #2 The threshold is documented and its interaction with history-based ordering is stated
- [x] #3 A decision on whether underpromotions are searched outside quiescence is recorded and implemented
- [x] #4 A decision on whether bad captures are subject to the same pruning is recorded
- [x] #5 Node counts at fixed depth are reduced on a representative position set, with figures recorded in the implementation notes
- [x] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a move-count late move pruning (LMP) step to the main search move loop (search.rs ~1583). Gate on: non-PV node, not in check, current move is in the history-ordered Quiet phase, remaining depth <= LMP_MAX_DEPTH, and move_count already searched >= a depth-indexed threshold late_move_count(depth). When it fires, break the remaining quiets for the node (bad captures still follow). The move counter (move_count) and OrderedMoves::phase already exist; no eval consulted.
2. Document the threshold constant/formula and state that LMP safety rests on history-based quiet ordering (activated by TASK-64.2), which is now always on.
3. Decision (AC#3): exclude underpromotions from the main search. They are the final ordering phase, always derived from a queen promotion that IS searched, so dropping them never removes the last legal move; keep them in quiescence. Implement by breaking the main move loop when the Underpromotions phase is reached.
4. Decision (AC#4): do NOT subject bad captures to LMP. Move-count pruning of losing captures is effectively main-search SEE pruning, which measured as a strong Elo regression previously; bad captures can still be tactical sacrifices. Record rationale in code + notes.
5. Add an lmp_enabled/lmp_disabled test hook mirroring lmr/futility hooks.
6. Tests: LMP shrinks the tree; LMP does not change a sound fixed-depth result vs disabled; underpromotions are excluded from the main search but still yielded in quiescence.
7. Measure node-count reduction at fixed depth on a representative set and run the TASK-27 strength script (fastchess nodes-limited match vs merge-base). Record figures in implementation notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented move-count late move pruning (LMP) plus underpromotion exclusion in engine/src/search.rs.

Design and decisions:
- Mechanism: at a non-PV, not-in-check node within LMP_MAX_DEPTH plies of the horizon, once move_count moves have been searched, each further move from the history-ordered Quiet phase is discarded with no re-search. Gated per node via `late_move_pruning`; applied per move after make_move.
- Threshold (AC#2): late_move_count(depth) = 3 + depth*depth/2, giving 3/5/7 moves at remaining depth 1/2/3. move_count counts all moves searched so far (hash move, captures, refutations precede quiets in the ordering), so the promising prefix is always kept and a capture-heavy node spends its allowance sooner. LMP's soundness rests on quiet moves being ordered by the history heuristic (activated in TASK-64.2, now always on): only history ordering makes a late quiet genuinely unpromising.
- Check exemption: a quiet move that gives check is never pruned — it is forcing and can deliver mate near the horizon. Whether a move checks is only known after it is made, so the prune sits just after make_move (mirroring futility pruning) rather than as a bare pre-make counter test.
- LMP_MAX_DEPTH = 3: a search tree is leaf-heavy, so pruning only within three plies of the horizon captures ~all the node saving (see figures) while leaving deeper mating/tactical lines fully searched. An earlier LMP_MAX_DEPTH = 8 variant pruned harder yet measured markedly weaker (+21 Elo vs +88.7 for the depth-3 cap) because it deferred/hid short forced mates; the depth-3 cap is both stronger and preserves the repo's fixed-depth mate invariants.
- Underpromotions (AC#3): excluded from the main search. They are the final ordering phase and each is derived from a queen promotion that is already searched, so dropping them never removes the last legal move (mate/stalemate detection stays sound). Quiescence is unchanged and still expands the queen-promotion segment into underpromotions. Implemented by breaking the move loop when the Underpromotions phase is reached.
- Bad captures (AC#4): deliberately NOT subject to LMP. Pruning losing captures by move count is in effect a static-exchange prune of the main search, which measured previously as a strong regression; a bad capture can also be a mating sacrifice. LMP is confined to the Quiet phase; the loop advances to and searches the BadCaptures phase normally.

Verification:
- cargo fmt --check: clean.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- cargo test --workspace: pass (all binaries). New tests: late_move_pruning_reduces_the_search_tree, late_move_pruning_keeps_a_decisive_capture, the_main_search_does_not_select_an_underpromotion. Also verified four pre-existing tactical-invariant tests still pass (gives_correct_answers, child_mate_windows_preserve_distance_parity, a_warm_table_matches_the_cold_result_and_never_costs_more_nodes, see_pruning_leaves_forced_results_unchanged); the check-exemption and depth-3 cap were required to keep these green.
- Node counts (AC#5), fixed depth 11, candidate vs merge-base baseline a5e52e6 (UCI `go depth 11`, nodes at last completed iteration):
    startpos    512108 -> 120631  (76.4%)
    middlegame1 1343661 -> 505812  (62.4%)
    middlegame2  111105 ->  76991  (30.7%)
    ruy          727432 -> 222106  (69.5%)
    endgame      142487 ->  75282  (47.2%)
    TOTAL       2836793 ->1000822  (64.7% fewer nodes)
- Strength (AC#6), fastchess, -each proto=uci restart=on nodes=100000, 500 games (250 rounds x2, repeat), openings tools/strength/openings-v1.epd, vs merge-base baseline a5e52e6:
    Elo +88.74 +/- 25.65, LOS 100.00%, 250/125/125 W/D-pairs, Ptnml [0,62,63,63,62], zero double-losses.

Base sha: a5e52e6 (merge-base with master). Implementation target: 28d1212.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-21 15:58
---
Implementation handoff
Branch: task-64.8-late-move-pruning
Worktree: /Users/seabo/seaborg-worktrees/task-64.8-late-move-pruning
Base: a5e52e604b0db0d87346785b1052a9bd268ac937
Implementation target: 28d12126e7b11e920398badd2bf9eb0e5112656c
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: pass (all binaries; includes 3 new LMP/underpromotion tests and 4 pre-existing tactical-invariant tests re-verified)
- Node reduction at fixed depth 11: 64.7% fewer nodes across a 5-position set (figures in notes)
- Strength (nodes=100000, 500 games vs merge-base): +88.74 +/- 25.65 Elo, LOS 100%
Known failures: none
---

author: @george
created: 2026-07-21 16:14
---
Review attempt: 1
Reviewed branch: task-64.8-late-move-pruning
Reviewed implementation: 28d12126e7b11e920398badd2bf9eb0e5112656c
Base: a5e52e604b0db0d87346785b1052a9bd268ac937
Verdict: approved

Immutability: base is the merge-base with master and an ancestor of the target; the only commit after the target (1525c61) touches solely the task file. Worktree clean.

Acceptance criteria:
- AC#1: LMP is gated non-PV, not-in-check, depth<=LMP_MAX_DEPTH(3), phase==Quiet, move_count>late_move_count(depth), with a per-move check exemption. Test late_move_pruning_reduces_the_search_tree proves it fires and shrinks the tree.
- AC#2: LMP_MAX_DEPTH and late_move_count are documented; the move-loop comment states LMP soundness rests on history-based quiet ordering.
- AC#3: underpromotions excluded from the main search (loop breaks at the Underpromotions phase), retained in quiescence; test the_main_search_does_not_select_an_underpromotion pins the behaviour. Mate/stalemate detection stays sound because each underpromotion derives from an already-searched queen promotion, so move_count>0 whenever a legal move exists.
- AC#4: bad captures deliberately not pruned; rationale recorded in code and notes; LMP is confined to the Quiet phase and the loop searches BadCaptures normally.
- AC#5: node-reduction figures recorded and independently reproduced at fixed depth 11 on startpos — base 512108 -> target 120631 (76.4% fewer), both engines returning e2e4 with identical PV and score.
- AC#6: strength recorded, +88.74 +/- 25.65 Elo over 500 nodes-limited games vs merge-base.

Verification (run on target 28d1212):
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings (fresh CARGO_TARGET_DIR): clean, no new #[allow]
- cargo test --workspace: pass (390 engine + others; 3 new tests + tactical invariants green)
- Node counts base a5e52e6 vs target 28d1212, startpos go depth 11: 512108 -> 120631, reproduced exactly
- Scope: only engine/src/search.rs and the task file changed; no external task/AC references in code comments
- Benchmarks: change is confined to the search move loop; the perft/movegen benches do not exercise search, so they carry no signal here; the reproduced 4x node reduction with preserved result is the relevant search-quality evidence.

The reviewed implementation SHA 28d12126e7b11e920398badd2bf9eb0e5112656c is the code target.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Move-count late move pruning plus underpromotion exclusion in engine/src/search.rs. In non-PV, not-in-check nodes within LMP_MAX_DEPTH=3 plies of the horizon, quiet moves past late_move_count(depth)=3+depth*depth/2 (3/5/7 at depth 1/2/3) are discarded with no re-search; checking quiets are exempt, bad captures are never pruned, underpromotions are dropped from the main search but retained in quiescence. Verified on target 28d1212: cargo fmt --check clean; cargo clippy --workspace --all-targets --all-features -- -D warnings clean (fresh CARGO_TARGET_DIR); cargo test --workspace passes including the three new tests (tree-reduction, decisive-capture-preserved, no-underpromotion) and the tactical-invariant suite. AC#5 independently reproduced at fixed depth 11 on startpos: base a5e52e6 512108 nodes -> target 120631 (76.4% fewer), both returning e2e4 with an identical PV and score. AC#6 recorded: +88.74 +/- 25.65 Elo over 500 nodes-limited games vs the merge-base.
<!-- SECTION:FINAL_SUMMARY:END -->
