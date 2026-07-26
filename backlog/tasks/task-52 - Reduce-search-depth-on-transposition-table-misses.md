---
id: TASK-52
title: Reduce search depth on transposition-table misses
status: Done
assignee:
  - '@george'
created_date: '2026-07-18 18:45'
updated_date: '2026-07-26 19:26'
labels: []
dependencies:
  - TASK-51
references:
  - engine/src/search.rs
priority: medium
type: feature
ordinal: 52000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search steps 11 and 13 are unimplemented placeholders. Both express the same idea - when no transposition-table move is available, the node is likely cheap to get wrong, so search it shallower - and differ only in node type and margin:

Step 11 (search.rs:604): in PV nodes, if the move is not in the TT, decrease depth by 3.
Step 13 (search.rs:610): in non-PV nodes with depth >= 7 and not in the TT, decrease depth by 2.

They are naturally paired and share one measurement. This depends on nothing in TASK-50 or TASK-51 mechanically; the dependency is purely sequencing, so that strength results remain attributable to one change at a time.

Note that TASK-12 repaired TT reuse and mate-score semantics, so TT hit and miss are now trustworthy signals to branch on.

The numbered Step N comments in search.rs are a deliberate map of the intended search structure. Replace the TODO markers with implementations; do not delete the step comments.

TODO sites: engine/src/search.rs:604, :610.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PV nodes with no transposition-table move are searched at reduced depth per step 11
- [x] #2 Non-PV nodes at depth >= 7 with no transposition-table move are searched at reduced depth per step 13
- [x] #3 Reduction is driven by a genuine TT miss and not by a collision-guard rejection, consistent with the semantics established by TASK-12
- [x] #4 Measured with the TASK-27 strength-regression script showing no strength loss, with results recorded in the implementation notes
- [x] #5 The step 11 and step 13 TODO markers are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make search_inner's depth parameter mutable.
2. Add IIR constants (IIR_PV_REDUCTION=3, IIR_NON_PV_REDUCTION=2, IIR_NON_PV_MIN_DEPTH=7) near the other search constants.
3. In Step 3's TT-probe match, track a tt_collision flag set only in the collision-guard branch, so IIR can distinguish a genuine miss/moveless entry from a Zobrist-collision rejection (AC#3, TASK-12 semantics).
4. Replace Step 11 TODO: PV node with no genuine TT move -> depth = (depth - 3).max(1). Floor at 1 because Step 5's quiescence handover is already behind us.
5. Replace Step 13 TODO: non-PV node, depth>=7, no genuine TT move -> depth -= 2.
6. Add iir_disabled test hook field + iir_enabled() gate, matching the rfp/lmr toggle convention.
7. Retain all numbered step comments; write reader-facing rationale.
8. Tests: IIR reduces the tree when it fires (PV and non-PV); IIR does not fire when a TT move is present; genuine-miss vs collision semantics covered by structure. Run fmt/clippy/test.
9. AC#4: run the TASK-27 strength-regression script; record results in notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented steps 11 and 13 (internal iterative reduction) in engine/src/search.rs.

Design:
- Step 11 (PV): depth = (depth - 3).max(1) when the node has no genuine TT move. Floored at 1 because Step 5's depth<=0 quiescence handover is already passed, so the move loop must keep >=1 ply.
- Step 13 (non-PV, depth>=7): depth -= 2. The depth>=7 gate keeps the reduced draft (>=5) clear of the quiescence handover, so no floor is needed.
- Constants IIR_PV_REDUCTION=3, IIR_NON_PV_REDUCTION=2, IIR_NON_PV_MIN_DEPTH=7. Step comments retained (AC#5).
- AC#3 (genuine miss vs collision): added a tt_collision flag set only in the Step-3 collision-guard branch (full-key hit whose move is unplayable here). Both IIR conditions require tt_mov.is_none() && !tt_collision, so a Zobrist collision on a foreign entry never triggers the reduction, consistent with TASK-12's decoupling of score and move. A miss or a legitimately move-less entry does trigger it.
- Added iir_enabled()/iir_disabled test hook matching the rfp/lmr toggle convention.

Tests (engine/src/search/tests.rs):
- internal_iterative_reduction_reduces_a_pv_search_tree: depth-6 PV search (below IIR_NON_PV_MIN_DEPTH so only step 11 can fire); IIR on visits strictly fewer nodes than off.
- internal_iterative_reduction_reduces_a_non_pv_search_tree: pure non-PV search at depth 7 (only step 13 can fire), other reductions disabled so the two-ply cut registers; IIR on < off.
- internal_iterative_reduction_ignores_a_transposition_collision: at depth 2 only the root's reduction is observable; a genuine miss reduces the tree, a seeded full-key entry with an unplayable move (collision) leaves the tree byte-identical to the un-reduced search. Directly exercises AC#3.
- Fixed two pre-existing depth-pinning tests by neutralising IIR the same way they already neutralise LMR/extensions: the_same_key_is_worth_different_scores_at_different_halfmove_clocks and a_repetition_derived_value_is_not_stored_in_the_table.
- gives_correct_answers KP-race entry: IIR trades horizon depth, so the won K+P-vs-K+P endgame's winning score surfaces at depth 24 rather than 22 (measured: cp 170 at d22, cp 795 at d24). The engine plays the winning move (a1b2) at every depth; only the promotion's full value lags. Raised that entry's depth 22->24 (score band [450,920] and moves unchanged), following the existing precedent for the extension-shifted rook-endgame entry.

AC#4 strength (authoritative): SPRT PASS. LLR 2.96, bounds +/-2.94, elo0=-5 elo1=0. Elo +28.3 +/- 10.8. 1524 games (405-838-281), pentanomial 25-124-353-222-38, 0 crashes, 0 forfeits. tc=8+0.08, 64MB hash, 1 worker/engine, concurrency 4. fastchess alpha 1.5.0 20251121-1eedf82, openings-v1.epd (sha256 eca44927...), target-cpu=native release. Baseline git:cfdac4d (merge-base), tested binary sha256 5bfc5223...; candidate tested binary sha256 4c646761.... Recorded in BENCHMARKS.md. Note: the candidate tested binary embeds GIT_HASH 82c1f3f (the claim commit; the IIR changes were in the working tree). GIT_HASH feeds only UCI version reporting (src/cmdline.rs), not search, so the tested binary's search behavior is byte-identical to a rebuild from the implementation target; verified two rebuilds from the target are reproducible (sha256 41788fc...) and differ from the tested binary by the embedded commit string alone.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (all suites green; engine lib 435 passed, 0 failed)
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-25 21:54
---
Implementation handoff
Branch: task-52-tt-miss-depth-reduction
Worktree: /Users/seabo/seaborg-worktrees/task-52-tt-miss-depth-reduction
Base: cfdac4d649599c9c5f117ada9b39dbc30110a875
Implementation target: 6c4a092
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass
- TASK-27 strength script (authoritative SPRT): PASS, +28.3 +/- 10.8 Elo, 1524 games, LLR 2.96 (bounds +/-2.94), tc=8+0.08; details in implementation notes and BENCHMARKS.md
Known failures: none
---

author: @george
created: 2026-07-25 22:22
---
Review attempt: 1
Reviewed branch: task-52-tt-miss-depth-reduction
Reviewed implementation: 6c4a092
Base: cfdac4d
Verdict: approved

All five acceptance criteria proven; no blocking findings.
- AC#1/#2/#5: steps 11 & 13 implemented (search.rs:2246, :2260), TODOs replaced, numbered step comments retained. Proven by tests internal_iterative_reduction_reduces_a_pv_search_tree and _reduces_a_non_pv_search_tree.
- AC#3: both cuts require tt_mov.is_none() && !tt_collision; tt_collision is set only in the Step-3 full-key-hit-with-unplayable-move branch, so a Zobrist collision never masquerades as a miss. Proven by internal_iterative_reduction_ignores_a_transposition_collision (collision run's node count equals the un-reduced tree; genuine miss reduces).
- AC#4: SPRT PASS, +28.3 +/- 10.8 Elo, 1524 games, LLR 2.96 (bounds +/-2.94), tc=8+0.08; recorded in notes and BENCHMARKS.md. No strength loss.

Scope: only search.rs, search/tests.rs, BENCHMARKS.md, task file. The gives_correct_answers K+P-race entry depth 22->24 is a justified horizon-trade accommodation (winning move unchanged at every depth; only the promotion's full value lags); two pre-existing depth-pinning tests neutralise IIR the same way they already neutralise LMR/extensions. Comments are reader-facing rationale with no bare task-ID references. #[allow] additions: none.

Verification on target 6c4a092 (code byte-identical to branch tip 12520e3; later commits are handoff/approval metadata only):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean CARGO_TARGET_DIR, exit 0)
- cargo test --workspace: green (chess 57, engine 435, lichess 156, integration suites pass)

Note: lichess run::tests::incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked flaked once under the full parallel run, then passed 3/3 in isolation. It is a timing/concurrency test in the lichess crate, which this diff does not touch; pre-existing environmental flake, not patch-introduced, non-blocking.

Perft/movegen speed benchmarks were not run: the diff changes only the alpha-beta search body, leaving move generation and perft byte-identical, so those benches cannot regress from it. Search hot-path performance is covered by the passing strength SPRT.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented search steps 11 & 13 (internal iterative reduction) in engine/src/search.rs: a PV node with no genuine TT move has its depth cut by IIR_PV_REDUCTION=3 (floored at 1), and a non-PV node at depth >= IIR_NON_PV_MIN_DEPTH=7 by IIR_NON_PV_REDUCTION=2. A tt_collision flag set only in the Step-3 collision guard makes both cuts fire on a genuine miss or move-less entry and never on a Zobrist-collision rejection (AC#3, TASK-12 semantics). The numbered step comments are retained (AC#5). Verified on implementation target 6c4a092: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings pass (clean CARGO_TARGET_DIR); cargo test --workspace green including three new IIR tests (PV cut, non-PV cut, collision-ignored). AC#4 strength: SPRT PASS +28.3 +/- 10.8 Elo, 1524 games, LLR 2.96 (bounds +/-2.94), tc=8+0.08, recorded in BENCHMARKS.md and notes.
<!-- SECTION:FINAL_SUMMARY:END -->
