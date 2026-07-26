---
id: TASK-86.1
title: Wire the NNUE accumulator to update incrementally in the search hot loop
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-25 12:22'
updated_date: '2026-07-26 22:31'
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
- [ ] #1 The accumulator is maintained incrementally across make/unmake during search (e.g. a per-ply accumulator stack mirroring the EvalState eval_stack), and read at Search::evaluate without a from-scratch rebuild
- [ ] #2 Null moves and any other move types that do not change piece placement are handled correctly (accumulator unchanged; only side-to-move / concatenation order flips)
- [ ] #3 Accumulator::from_position is retained as a from-scratch equivalence reference, and a test asserts the incrementally-maintained accumulator is bit-identical to a full rebuild across a representative position/move suite including captures, promotions, castling, en passant, and null moves
- [ ] #4 Evaluation output is bit-identical before and after this change (verified by a fixed-nodes search producing identical results)
- [ ] #5 Before/after single-thread bench NPS is measured under controlled conditions and recorded in BENCHMARKS.md with attribution
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Store the incremental accumulator's activation payload ([Box<[i16]>;2], lifetime-free) live in Search plus a restore stack mirroring eval_stack. Seed it from Accumulator::from_position(net,pos) when a network is selected; None on the hand-crafted path. Add additive Accumulator::from_values / into_values helpers (borrowing API for from_position/seeded/tests stays untouched — avoids the self-referential-struct problem since Search owns Arc<Network>).
2. Wire the six make/unmake wrappers: make_move/make_move_checked push a clone of the live payload then replay_last_move_deltas folds the move into a borrowing Accumulator wrapped around the live payload; unmake_move pops-restores (copy, not reverse-delta). make_null_move/unmake_null_move carry the payload unchanged. All gated on network presence; hand-crafted eval_state path untouched.
3. Read the maintained accumulator at Search::evaluate (network branch) instead of Accumulator::from_position. Keep static_eval (external position) from-scratch. Re-seed in Search::set_network so a post-construction network change stays consistent.
4. Retain Accumulator::from_position as the from-scratch reference; keep per-node debug_asserts (sync_after_make, null, evaluate) equating maintained payload to a full rebuild. Add a search-seam test walking make/unmake incl captures/promotions/castling/en passant/null moves asserting bit-identical to from_position, and a fixed-depth network-search determinism/equivalence test.
5. Run required checks. Measure single-thread bench NPS before/after with the built-in network (round-robin, idle machine) and record in BENCHMARKS.md with attribution.
<!-- SECTION:PLAN:END -->
