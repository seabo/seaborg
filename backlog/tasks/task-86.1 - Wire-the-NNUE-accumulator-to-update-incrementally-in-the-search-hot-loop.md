---
id: TASK-86.1
title: Wire the NNUE accumulator to update incrementally in the search hot loop
status: Done
assignee:
  - '@george'
created_date: '2026-07-25 12:22'
updated_date: '2026-07-27 09:49'
labels:
  - nnue
dependencies: []
parent_task_id: TASK-86
priority: high
ordinal: 143000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Today Search::evaluate rebuilds the accumulator from scratch at every evaluated leaf via Accumulator::from_position (engine/src/search.rs), an O(pieces x H) cost per node. The incremental PieceDeltaSink accumulator (TASK-69.3) and the make/unmake evaluation seam (TASK-64.15) already exist but are not wired into the search hot loop. Maintain the accumulator incrementally along make/unmake through search so per-node eval cost becomes O(features-toggled x H) and is nearly independent of piece count. This is a pure speed change with bit-identical eval output, and it is the prerequisite that makes a wider or king-bucketed network affordable without cratering NPS; the payoff grows with hidden width H.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The accumulator is maintained incrementally across make/unmake during search (e.g. a per-ply accumulator stack mirroring the EvalState eval_stack), and read at Search::evaluate without a from-scratch rebuild
- [x] #2 Null moves and any other move types that do not change piece placement are handled correctly (accumulator unchanged; only side-to-move / concatenation order flips)
- [x] #3 Accumulator::from_position is retained as a from-scratch equivalence reference, and a test asserts the incrementally-maintained accumulator is bit-identical to a full rebuild across a representative position/move suite including captures, promotions, castling, en passant, and null moves
- [x] #4 Evaluation output is bit-identical before and after this change (verified by a fixed-nodes search producing identical results)
- [x] #5 Before/after single-thread bench NPS is measured under controlled conditions and recorded in BENCHMARKS.md with attribution
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Store the incremental accumulator's activation payload ([Box<[i16]>;2], lifetime-free) live in Search plus a restore stack mirroring eval_stack. Seed it from Accumulator::from_position(net,pos) when a network is selected; None on the hand-crafted path. Add additive Accumulator::from_values / into_values helpers (borrowing API for from_position/seeded/tests stays untouched — avoids the self-referential-struct problem since Search owns Arc<Network>).
2. Wire the six make/unmake wrappers: make_move/make_move_checked push a clone of the live payload then replay_last_move_deltas folds the move into a borrowing Accumulator wrapped around the live payload; unmake_move pops-restores (copy, not reverse-delta). make_null_move/unmake_null_move carry the payload unchanged. All gated on network presence; hand-crafted eval_state path untouched.
3. Read the maintained accumulator at Search::evaluate (network branch) instead of Accumulator::from_position. Keep static_eval (external position) from-scratch. Re-seed in Search::set_network so a post-construction network change stays consistent.
4. Retain Accumulator::from_position as the from-scratch reference; keep per-node debug_asserts (sync_after_make, null, evaluate) equating maintained payload to a full rebuild. Add a search-seam test walking make/unmake incl captures/promotions/castling/en passant/null moves asserting bit-identical to from_position, and a fixed-depth network-search determinism/equivalence test.
5. Run required checks. Measure single-thread bench NPS before/after with the built-in network (round-robin, idle machine) and record in BENCHMARKS.md with attribution.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the incremental NNUE accumulator in the search hot loop.

Design: Search now holds the accumulator's activation payload live (Option<[Box<[i16]>;2]>) plus a flat restore stack (Vec<i16>), mirroring the eval_state/eval_stack seam (TASK-64.15). The borrowing Accumulator<'net> API is unchanged; two additive helpers Accumulator::from_values/into_values pair the lifetime-free payload with the network only for the instant a move is folded in or a leaf is scored. Storing the payload lifetime-free avoids a self-referential Search (it owns the network behind an Arc).
- evaluate (network branch) reads the maintained accumulator via forward() instead of Accumulator::from_position.
- make_move/make_move_checked/make_null_move push the pre-move payload; sync_eval_after_make folds the move in with replay_last_move_deltas; unmake pops-restores by copy. Null moves carry the payload unchanged.
- The restore stack is one pre-sized flat i16 buffer (append into spare capacity), not a Vec of boxed pairs: the latter heap-allocated twice per node and cancelled the whole NPS gain.
- static_eval keeps its from-scratch path (it scores an arbitrary external position). set_network re-seeds the accumulator from the root.
- The expensive per-node Accumulator::from_position debug_assert was intentionally not added inline: at O(pieces x H) per interior node it slowed debug searches enough to blow an unrelated wall-clock deadline test. Equivalence is instead verified exhaustively by the walk tests; the cheap O(pieces) EvalState debug_assert is retained.

Tests: search_maintains_the_accumulator_bit_identically_to_a_rebuild walks the real make/unmake/null seam over representative subtrees (captures, promotions, castling both sides, en passant, null moves) asserting bit-identity to Accumulator::from_position (release-valid assert_eq). fixed_depth_network_search_is_deterministic exercises the integrated network search. Existing accumulator.rs and inference.rs suites unchanged.

AC#4 evidence: at fixed depth the search visits identical node counts base vs branch on every measured position (150749/367085/142714/49991), an empirical bit-identity of the eval-driven tree.

AC#5 measurement (single thread, built-in gen-002 net H=256, go depth 12, Apple M3 Pro, rustc 1.97.1, idle, medians of interleaved base 9fe845c vs branch runs): aggregate 435,913 -> 571,632 nps (+31%), time 1630->1243ms, node counts identical. startpos +65%, kiwipete +27%, middlegame +25%, endgame ~flat (timer-noise-dominated). Recorded in BENCHMARKS.md. Gain is on the scalar ARM NNUE path and grows with H.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-26 23:19
---
Implementation handoff
Branch: task-86.1-incremental-accumulator
Worktree: /Users/seabo/seaborg-worktrees/task-86.1-incremental-accumulator
Base: 9fe845c8e8ce7eea54e455687eed366ef324aa1f
Implementation target: d1da68eb714158eca1862cecd0fba4b817947f19
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (engine 437, chess 57, lichess 157, +others; 0 failed, 2 ignored)
- Single-thread NPS bench (go depth 12, built-in gen-002 net): +31% aggregate (435,913 -> 571,632 nps), node counts bit-identical base vs branch; recorded in BENCHMARKS.md
Known failures: none
---

author: @george
created: 2026-07-27 09:14
---
Review attempt: 1
Reviewed branch: task-86.1-incremental-accumulator
Reviewed implementation: d1da68eb714158eca1862cecd0fba4b817947f19
Verdict: approved

Immutability: base 9fe845c is an ancestor of target d1da68e, which is an ancestor of tip b8d6997. The only commit after the target (b8d6997) touches nothing but the task file (handoff metadata). No implementation file changed after the reviewed SHA.

Acceptance criteria (all proven):
- AC#1: accumulator + flat accumulator_stack fields maintained across the six make/unmake/null wrappers; evaluate() reads the maintained payload via Accumulator::from_values instead of from_position. Confirmed no path mutates self.pos outside the wrappers.
- AC#2: make_null_move carries the payload unchanged; the forward pass reads pos.turn() for concatenation order. Asserted before/after null in the walk test.
- AC#3: Accumulator::from_position retained as the from-scratch reference; search_maintains_the_accumulator_bit_identically_to_a_rebuild walks the real seam over opening/kiwipete/en-passant/promotion positions (captures, promotions, castling both sides, en passant, null moves) with release-valid assert_eq.
- AC#4: maintained payload is bit-identical to from_position at every node and forward() is a pure function of (network, payload, turn), so leaf values and the whole fixed-depth tree are unchanged; fixed_depth_network_search_is_deterministic pins the integrated path.
- AC#5: before/after single-thread NPS recorded in BENCHMARKS.md with full attribution (M3 Pro, rustc 1.97.1, gen-002 H=256, interleaved medians). Independently corroborated: identical fixed-depth startpos search ~1025 ms at base vs ~730 ms at target.

Verification (run on target d1da68e in the task worktree):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean under a fresh CARGO_TARGET_DIR
- cargo test --workspace: pass (engine 437, chess 57, lichess 157, +others; 0 failed, 2 ignored); both new tests pass
- Relative NPS probe base 9fe845c vs target d1da68e: ~1.4x faster, node counts identical by construction

Scope: diff is confined to the two additive accumulator helpers, the search wiring, the two new tests, BENCHMARKS.md, and the task file. No new #[allow], no unrelated changes. Non-blocking observation: the 'every node rebuilds the accumulator from scratch' rationale in fixed_depth_network_search_is_deterministic reads most naturally as the O(pieces) EvalState debug guard, since the NNUE accumulator is deliberately not rebuilt per node — not a defect, noted only for a future touch.

Code target for merge: d1da68eb714158eca1862cecd0fba4b817947f19.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Wired the NNUE first-layer accumulator to update incrementally along search make/unmake instead of rebuilding it from scratch (O(pieces x H)) at every leaf. Search now holds the activation payload live (Option<[Box<[i16]>;2]>) plus a flat pre-sized i16 restore stack mirroring the eval_stack seam; two additive Accumulator::from_values/into_values helpers pair the lifetime-free payload with the Arc-held network only to fold a move in or score a leaf, avoiding a self-referential Search. make/unmake/null wrappers push/pop the payload; evaluate reads the maintained accumulator. Bit-identity to a from-scratch rebuild is proven per-node over representative subtrees (captures, promotions, castling, en passant, null moves) by search_maintains_the_accumulator_bit_identically_to_a_rebuild (release-valid assert_eq), with fixed_depth_network_search_is_deterministic pinning the integrated path. Verified on target d1da68e: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -D warnings clean under a fresh CARGO_TARGET_DIR; cargo test --workspace pass (engine 437 / chess 57 / lichess 157, 0 failed). Reviewer independently confirmed the speed win: identical fixed-depth startpos search ran ~1025 ms at base 9fe845c vs ~730 ms at target (node counts identical by construction), corroborating the +31%% aggregate NPS recorded in BENCHMARKS.md.
<!-- SECTION:FINAL_SUMMARY:END -->
