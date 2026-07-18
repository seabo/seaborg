---
id: TASK-36
title: Fix illegal moves in the reported principal variation (info ... pv ...)
status: Done
assignee:
  - '@claude'
created_date: '2026-07-18 01:21'
updated_date: '2026-07-18 13:51'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: high
type: bug
ordinal: 41000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
seaborg reports principal variations over UCI (info ... pv ...) that contain illegal moves. Reproduced authoritatively in TASK-34 with FastChess self-play at depth=4: 'Warning; Illegal PV move - move c5f8' for the line 'info depth 4 multipv 1 score mate -2 ... pv d7f8 g6a6 f8g6 c5f8'. The reported best move (first PV ply) is legal — games complete with correct results — so this is a PV-reporting defect, not a move-selection defect; the corruption is on deeper PV plies and shows up on mate-scored / shallow lines emitted during play.

Root cause (from TASK-34, doc-2): the PV shown over UCI is rebuilt from the triangular PVTable (engine/src/pv_table.rs) via emit_progress -> pvt.pv() (search.rs:931-941). The table is updated on every alpha-raise, including fail-high/beta-cutoff nodes: search.rs:671-698 calls pvt.copy_to(depth, mov) in the value>=beta branch before break 'move_loop. copy_to/update_internal splice the child row into the parent via copy_within, but on a cutoff (and around mate/leaf handling via pv_leaf_at) the child row can still hold moves from a different sibling subtree, which get copied up — so the reconstructed line does not chain legally beyond the first move.

Scope: ensure every move in the reported PV is legal in the position reached by playing the preceding PV moves, without changing which move the engine selects/plays. Fix the PV reconstruction so stale sibling entries and mate/leaf handling cannot splice illegal continuations (e.g. only update the PV on exact/alpha-raising PV nodes, correctly clear/propagate child rows, or reconstruct the PV from validated data). This defect is independent of the completion-deadlock and EOF defects and of TASK-32.

Relevant code: engine/src/pv_table.rs (copy_to/update_internal/pv_leaf_at/pv), engine/src/search.rs (Step 22 PV update on cutoffs, mate/stalemate leaf handling), engine/src/info.rs (format_search_event). See backlog doc-2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every move in every reported 'info ... pv ...' line is legal in the position reached after playing the preceding PV moves, including on mate/stalemate-scored and beta-cutoff lines
- [x] #2 Validated with FastChess (or cutechess) seaborg self-play at fixed depth: zero 'Illegal PV move' warnings across a multi-game match
- [x] #3 A regression test drives the search on positions that previously produced illegal PVs (including the mate line d7f8 g6a6 f8g6 c5f8) and asserts the full reported PV is legal by playing it out
- [x] #4 The engine's selected/played best move is unchanged by the fix; existing search-correctness tests (e.g. gives_correct_answers) still pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a PV-legality test harness that runs the search with an event channel, collects every emitted 'info ... pv ...' line, and replays each PV from the root position asserting every move is legal. Confirm it reproduces illegal PVs on the current code (capture concrete FEN/depth).
2. Fix the PV table so stale sibling rows can never be spliced up:
   - Clear this ply's PV row on entry to `search`, before any early return (TT/draw/mate-distance/razoring/stopping), so a node that returns without establishing a PV leaves an empty row instead of a previous sibling's line.
   - Only update the PV on exact PV-node alpha raises (move the `pvt.copy_to` call inside the `Node::pv() && value < beta` branch), so fail-high/beta-cutoff nodes no longer publish non-exact lines. Root is a PV node with beta = INF_P, so the root move is unaffected.
   - Retire the now-redundant `pv_leaf_at` in favour of the clear-on-entry invariant.
3. Add pv_table unit tests for row clearing and truncation semantics.
4. Add the regression test from step 1 over mate-scored and tactical positions across depths 1..=N.
5. Verify: cargo test workspace, cargo clippy, cargo fmt; confirm search suite (gives_correct_answers) unchanged; run FastChess seaborg self-play at fixed depth and confirm zero 'Illegal PV move' warnings.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause confirmed as two independent PVTable defects, both required for a legal PV:

1. Rows were never reset between siblings. A node that returned without establishing a line — transposition cutoff, immediate draw, mate-distance prune, razoring, abort, checkmate/stalemate, or an all-node fail-low — left the previously searched sibling's continuation in its row, and the parent's copy_to spliced that unrelated line into its own PV. The task description and doc-2 named only the beta-cutoff path; the stale-row path is the more general cause and is what the mate/leaf handling interacted with.
2. copy_to ran on every alpha-raise, including at fail-high nodes. A fail-high returns a lower bound whose best move was never searched with a full window, so its continuation is not a PV.

Fix: PVTable::pv_leaf_at is generalised to PVTable::clear_at and called on entry to every search node, before any early return; copy_to moved inside the 'Node::pv() && value < beta' branch. The mate/stalemate clear at Step 23 and the dead quiescence-mate clear at Step 5 are subsumed by the clear-on-entry invariant and were removed.

Move selection is unchanged by construction: the root is a PV node searched with beta = INF_P and value is asserted strictly below INF_P, so the root always takes the exact-alpha-raise branch. best_move, did_raise_alpha, and the transposition-table write are untouched; the diff only gates which lines are published for reporting.

Verification evidence:
- Reproduction pinned before fixing: the FastChess self-play game record whose final position emits the reported line is replayed verbatim in the regression test. Against the unfixed logic it fails with 'illegal PV move at ply 4 (c5f8) of depth-4 pv [d7f8 g6a6 f8g6 c5f8]' — the exact line from doc-2. The move list is used rather than the equivalent FEN because the repetition history it builds is part of what the search sees.
- FastChess self-play, depth=4, 40 games, identical conditions on both binaries: master 87d5218 emits 40 'Illegal PV move - move c5f8' warnings; this branch emits 0. Both matches produce identical results (40 decisive, Ptnml [0,0,20,0,0]), consistent with move selection being unchanged.

Pre-existing unrelated failure: search::tests::fifty_move_rule_uses_halfmove_boundary panics with 'attempt to divide by zero' at engine/src/trace.rs:141 (Tracer::live_nps divides by elapsed micros, which is 0 when a search completes in under a microsecond). This reproduces on master 87d5218 in release builds — 5/5 runs in isolation, 1/3 full-suite runs — and equally on this branch. The debug suite is unaffected because the search is slow enough there. Out of scope for TASK-36 and not introduced by it.

Drive-by fix folded in at user request (2026-07-18), superseding the 'known failure' recorded in the first handoff.

engine/src/trace.rs measured search durations in whole microseconds, so a search completing faster than that measured as zero and both NPS accessors panicked with 'attempt to divide by zero'. Reachable from emit_progress, which builds a SearchProgress unconditionally, and the cause of search::tests::fifty_move_rule_uses_halfmove_boundary failing in release builds on master.

The shared arithmetic is extracted into a private nps_over(nodes, elapsed) used by both live_nps and nps, charging a sub-microsecond search one microsecond. The new unit tests call nps_over directly rather than racing a real search: a first attempt asserted on Tracer timing end-to-end and was itself flaky under load, which is the same defect shape as the failure being fixed.

Evidence: fifty_move_rule_uses_halfmove_boundary now passes 5/5 runs in isolation in release, where master 87d5218 fails 5/5; the full release workspace suite passes 5/5 consecutive runs, where master flaked 1/3. This is a behavioural fix outside the task's acceptance criteria and should be reviewed on its own merits.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-18 12:53
---
Implementation handoff
Branch: task-36-illegal-pv-moves
Worktree: /Users/seabo/seaborg-worktrees/task-36-illegal-pv-moves
Base: 87d52189030611a2b23f357bd36e91b1b4e7790f
Implementation target: d04d3a430357401e3d680f87b0e21c204b301312
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo test --workspace (debug): pass, 79 engine + 35 core + 5 build-metadata + 1 doc-test, 0 failed
- cargo test --workspace --release: pass except the pre-existing failure below
- cargo test -p engine --release reported_principal_variations_are_legal: pass; fails on the unfixed logic with 'illegal PV move at ply 4 (c5f8) of depth-4 pv [d7f8 g6a6 f8g6 c5f8]'
- cargo test -p engine --release pv_table: pass, 4 new unit tests
- cargo test -p engine --release gives_correct_answers: pass, best moves and score bounds unchanged across the search suite
- fastchess -engine cmd=<target/release/seaborg> args=-u -engine cmd=<same> args=-u -each proto=uci depth=4 -rounds 20 -games 2 -concurrency 4: 0 'Illegal PV move' warnings over 40 games (master 87d5218 under the identical command: 40 warnings, all 'move c5f8')
Known failures: search::tests::fifty_move_rule_uses_halfmove_boundary panics 'attempt to divide by zero' at engine/src/trace.rs:141 in release builds. Pre-existing and unrelated: on master 87d5218 it fails 5/5 runs in isolation ('cargo test -p engine --release fifty_move_rule_uses_halfmove_boundary') and 1/3 full-suite runs; this branch behaves identically. Tracer::live_nps divides by elapsed micros, which is 0 for sub-microsecond searches. Not introduced by this change and out of scope for TASK-36.
---

author: @claude
created: 2026-07-18 13:07
---
Implementation handoff (supersedes the handoff naming target d04d3a4)
Branch: task-36-illegal-pv-moves
Worktree: /Users/seabo/seaborg-worktrees/task-36-illegal-pv-moves
Base: 87d52189030611a2b23f357bd36e91b1b4e7790f
Implementation target: fa13e80415671371e8bb9ccb49d914943e961cd9
Resolved findings: none

Scope note: fa13e80 is a drive-by fix to engine/src/trace.rs folded in at user request. It is outside this task's acceptance criteria and fixes the divide-by-zero previously reported as a known pre-existing failure. Please review it on its own merits. The PV fix proper is d04d3a4 and is unchanged by it.

Verification:
- cargo fmt --check: pass
- cargo test --workspace (debug): pass, 81 engine + 35 core + 5 build-metadata + 1 doc-test, 0 failed
- cargo test --workspace --release: pass, 5/5 consecutive runs (master 87d5218 flakes 1/3 on fifty_move_rule_uses_halfmove_boundary)
- cargo test -p engine --release fifty_move_rule_uses_halfmove_boundary: pass 5/5 in isolation (master 87d5218: fails 5/5, 'attempt to divide by zero' at engine/src/trace.rs:141)
- cargo test -p engine --release reported_principal_variations_are_legal: pass; fails on the unfixed logic with 'illegal PV move at ply 4 (c5f8) of depth-4 pv [d7f8 g6a6 f8g6 c5f8]'
- cargo test -p engine --release pv_table: pass, 4 unit tests
- cargo test -p engine --release trace: pass, 2 unit tests
- cargo test -p engine --release gives_correct_answers: pass, best moves and score bounds unchanged across the search suite
- fastchess -engine cmd=<target/release/seaborg> args=-u -engine cmd=<same> args=-u -each proto=uci depth=4 -rounds 20 -games 2 -concurrency 4: 0 'Illegal PV move' warnings over 40 games, re-run against fa13e80 (master 87d5218 under the identical command: 40 warnings, all 'move c5f8'); both matches 40 decisive, Ptnml [0,0,20,0,0]
Known failures: none
---

author: @claude
created: 2026-07-18 13:38
---
Review attempt: 1
Reviewed branch: task-36-illegal-pv-moves
Reviewed implementation: fa13e80415671371e8bb9ccb49d914943e961cd9
Base: 87d52189030611a2b23f357bd36e91b1b4e7790f
Verdict: approved

Target immutability: fa13e80 is an ancestor of the branch tip 7853e9d, and the only commit after it touches the task file. Worktree clean.

Scope: engine/src/pv_table.rs, engine/src/search.rs, engine/src/trace.rs, task file. No unrelated changes. Reviewed the invariant behind the fix: search is only ever entered at depth <= pvt.depth (root at search_depth, recursion at depth-1, no extensions or reductions implemented), so clear_at cannot underflow; the root is a PV node with beta = INF_P and mate scores cap at 20_100, so the root always takes the exact-alpha-raise branch and best_move is structurally unchanged.

AC #1 - PV legality. Independent randomized sweep over 60 games worth of positions at depths 1-6 validated 1,260 reported PV lines / 3,325 moves with python-chess: 0 illegal. engine/src/info.rs is a pure passthrough of SearchProgress.principal_variation, so the test's data source is exactly what the UCI line prints. No empty pv fields emitted.

AC #2 - self-play. Independently re-run, not taken from the handoff: fastchess seaborg-vs-seaborg, 40 games at depth 4 -> 0 'Illegal PV move' warnings, results 40 decisive, Ptnml [0,0,20,0,0]. Extended to 150 further games at depths 3, 5 and 6 -> 0 warnings. Counterfactual on the same machine with only the two production hunks reverted: 40/40 games emit 'Illegal PV move - move c5f8'.

AC #3 - regression test. Confirmed the test genuinely fails without the fix, not merely that it exists: reverting the clear_at call and the copy_to gating in a throwaway worktree makes reported_principal_variations_are_legal fail with 'illegal PV move at ply 4 (c5f8) of depth-4 pv [d7f8 g6a6 f8g6 c5f8]' in 8/5P2/R5n1/4k1P1/P1Q5/8/4N3/4K3 w - - 3 55 - the exact line from the task description.

AC #4 - move selection unchanged. bestmove identical between base and target on 22 positions x depths 1-8 (176 searches) and on 22 positions searched to depth 8 with all iteration info lines captured; scores identical on every info line; depth-9 node counts byte-identical on three positions (8,350,076 / 1,781,513 / 218,824). gives_correct_answers and the full suite pass.

Drive-by trace.rs fix, reviewed on its own merits as the handoff requested: base 87d5218 fails fifty_move_rule_uses_halfmove_boundary 5/5 in release with 'attempt to divide by zero' at trace.rs:141; target passes 5/5. nps_over is correct for the boundary cases its unit tests cover and cannot overflow usize at realistic node counts. Testing the helper directly rather than racing a real search is the right call.

Non-blocking observations, recorded rather than deferred as findings:
- Mate-scored PVs are now one ply shorter (e.g. 'pv c7c6 a6a5 f4c7' becomes 'pv c7c6 a6a5'). All 39 changed info lines across the comparison were mate-scored; non-mate PVs are byte-identical to base. This is the inherent trade-off of publishing only exact PV-node lines, which the task description proposes, and a truncated PV is legal UCI. It is not called out in the implementation notes.
- pv_table.rs places the tests module between the PVIter impl and the Debug impl rather than at the end of the file. Cosmetic.

Verification:
- git merge-base --is-ancestor fa13e80 HEAD: pass; git diff --stat fa13e80 HEAD: task file only
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets: pass, no warnings in the changed files
- cargo test --workspace: pass (35 core + 81 engine + 5 build-metadata + 1 doc-test, 0 failed)
- cargo test --workspace --release: pass, 0 failed
- cargo test -p engine --release fifty_move_rule_uses_halfmove_boundary: pass 5/5 (base: fail 5/5)
- fastchess depth=4 40 games: 0 warnings (base and fix-reverted build: 40 warnings)
- fastchess depths 3/5/6, 50 games each: 0 warnings
- cargo bench --bench perft --bench movegen, base vs target on the same idle-enough machine: generate moves 193.24 ns -> 191.49 ns, perft 5 22.295 ms -> 22.506 ms; confidence intervals overlap and the diff contains no movegen or perft code, so this is noise
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reported principal variations are now always legal. Two independent PVTable defects are fixed in engine/src/{pv_table,search}.rs: rows are cleared on entry to every search node (PVTable::pv_leaf_at generalised to clear_at), so a node returning early can no longer leave a sibling's line for its parent to splice up; and pvt.copy_to is moved inside the exact PV-node alpha-raise branch, so fail-high nodes no longer publish non-PV continuations. engine/src/trace.rs additionally gains a nps_over helper that charges a sub-microsecond search one microsecond, fixing a divide-by-zero panic reachable from emit_progress.

Verified at fa13e80: cargo test --workspace passes in debug (35 core + 81 engine + 5 build-metadata + 1 doc) and release; cargo fmt --check and cargo clippy clean on the changed files. The committed regression test reported_principal_variations_are_legal fails with the exact documented line 'illegal PV move at ply 4 (c5f8) of depth-4 pv [d7f8 g6a6 f8g6 c5f8]' when the fix is reverted. FastChess self-play: 0 'Illegal PV move' warnings over 190 games at depths 3-6, where the base commit emits 40 warnings over 40 games at depth 4. Move selection is unchanged: bestmoves and scores are identical to base 87d5218 across 22 positions x depths 1-8, and depth-9 node counts are byte-identical on three positions. perft/movegen benchmarks are within noise of base on the same machine.
<!-- SECTION:FINAL_SUMMARY:END -->
