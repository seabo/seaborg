---
id: TASK-64.15
title: Add an incremental evaluation seam to make and unmake
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-19 13:34'
updated_date: '2026-07-19 22:20'
labels:
  - evaluation
  - nnue
  - architecture
  - performance
dependencies:
  - TASK-64.14
references:
  - core/src/position/mod.rs
  - engine/src/eval.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: enhancement
ordinal: 78000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The evaluation is recomputed from scratch at every call, and there is no hook through which an incrementally maintained evaluation state could be updated. An NNUE accumulator requires one. Establish that seam now, with the tapered hand-crafted evaluation as its first consumer, so that adding a network later is a substitution into an existing mechanism rather than a new mechanism plus a migration.

Current state. `material_evaluation` (eval.rs:32-43) reads bitboards and recomputes on every call. `make_move`, `make_move_unchecked` and `unmake_move` in core maintain incremental state for the Zobrist key already, so the pattern exists in the codebase, but no evaluation state participates in it and there is no place for one to.

Why this is scheduled after the tapered evaluation rather than before. Material and piece-square scores are the canonical incremental terms and are simple enough to validate exhaustively against a from-scratch recomputation. Building the seam against a real consumer establishes the update, undo and correctness-check pattern under conditions where the reference answer is cheap to compute. An NNUE accumulator has the same shape and much more expensive validation.

Design questions to settle and record: where the incremental state lives given that `Search` owns its `Position` by value; how state is restored on unmake, whether by recomputation, a stored delta or a per-ply stack; how null moves interact with it once TASK-50 introduces them, since a null move changes side to move without moving a piece; and how the seam behaves across the copy that occurs when a search is started on a cloned position.

The correctness requirement is absolute and cheap to test here: an incrementally maintained evaluation must equal a from-scratch recomputation at every node. That equivalence should be asserted under debug builds throughout the search, not only in unit tests, because the failure mode is a slow divergence that unit tests on short move sequences will not surface.

This task delivers the seam and the hand-crafted evaluation's use of it. It does not deliver an NNUE accumulator, network format, or inference.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluation state is updated incrementally on make and unmake rather than recomputed from scratch on every evaluate call
- [ ] #2 A debug-build assertion verifies that the incrementally maintained evaluation equals a from-scratch recomputation at every node
- [ ] #3 The restoration strategy on unmake is documented and covered by tests including deep move sequences
- [ ] #4 The interaction with null moves is defined, or the absence of null moves at the time of implementation is recorded together with the constraint it places on TASK-50
- [ ] #5 The seam behaves correctly when a position is cloned to start a search
- [ ] #6 A benchmark records the change in nodes per second against the from-scratch baseline
- [ ] #7 The design is documented sufficiently that an NNUE accumulator can be added as a further consumer without reworking the seam
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a piece-delta seam to core: a `PieceDeltaSink` trait (add/remove piece at square) and `Position::replay_last_move_deltas`, which replays the board changes of the most recently made move onto a sink using core's existing move geometry (capture, en passant, castling, promotion). This keeps evaluation weights out of core and gives the future NNUE accumulator the same entry point.
2. Add `EvalState` to engine::eval: a White-relative { mg, eg, phase } accumulator with add_piece/remove_piece primitives (sharing the exact PST arithmetic), `from_position` (from-scratch reference), and `score()` (tapered interpolation). Implement `PieceDeltaSink` for it. Refactor `tapered_evaluation` to `EvalState::from_position(pos).score()` so there is one source of truth.
3. Wire the seam into Search: hold `eval_state: EvalState` (rebuilt from the position in `build`, so a cloned start position is correct by construction) plus an `eval_stack` for O(1) restore. Add make/unmake wrappers that update the accumulator incrementally on make and restore it from the stack on unmake, and replace the raw pos.make/unmake call sites. `evaluate()` reads `eval_state.score()`.
4. Correctness: a debug assertion after every make compares the incremental accumulator to a full from-scratch recomputation (fires at every node, not only in unit tests); `evaluate()` asserts the consumed score equals the from-scratch score.
5. Null moves: none exist yet (TASK-50). Document that a null move moves no piece, so the White-relative accumulator is unchanged across it and must simply be saved/restored; record this constraint for TASK-50.
6. Tests: deep move-sequence make/unmake round-trip equivalence, cloned-position correctness, and per-node incremental==from-scratch over a real search.
7. Benchmark: round-robin master vs branch on the search benches; record the NPS change and attribution.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Built the seam in three layers.

**core (`PieceDeltaSink` + `Position::replay_last_move_deltas`).** A move is reported to a consumer as the exact add/remove piece placements it makes to the board — the moving piece leaving its origin and arriving at its destination (as the promoted piece on a promotion), a capture removing the taken piece (behind the destination for en passant), and castling adding the rook's move. Core owns the move geometry; the consumer stays ignorant of it, and core stays ignorant of evaluation weights. This is the entry point a future NNUE accumulator reuses rather than a hand-crafted-eval-specific hook.

**engine (`EvalState`).** A White-relative { mg, eg, phase } accumulator implementing `PieceDeltaSink`. `from_position` (from-scratch) and the incremental path share a single `term()` for the per-piece arithmetic, so they cannot drift. `tapered_evaluation` now delegates to `EvalState::from_position(pos).score()`, making it the single source of truth for the tapered formula.

**search.** `Search` holds `eval_state` in step with `pos`, seeded from the (possibly cloned) start position in `build` — so a search started on a clone is correct by construction (AC#5). make/unmake wrappers update the accumulator incrementally on make and restore it on unmake from an O(1) per-ply `eval_stack` (AC#1, AC#3). `evaluate()` consumes `eval_state.score()`.

**Correctness (AC#2).** `sync_eval_after_make` asserts `eval_state == EvalState::from_position(pos)` after every make, and `evaluate()` asserts the consumed score equals `static_eval()`. Both are `debug_assert`, so they run throughout every debug-build search (including the whole test suite) and are compiled out of release. A slow divergence surfaces at the node it happens on.

**Null moves (AC#4).** None exist yet (TASK-50 introduces them). The accumulator is side-independent (White-relative), so a null move — which changes only the side to move — leaves it unchanged; it must simply be saved/restored across one. `replay_last_move_deltas` debug-asserts it is not called for a null move, forcing TASK-50 to decide that carry-across consciously. Documented on the trait, on `EvalState`, and on the method.

**Tests (AC#3, AC#5).** `incremental_evaluation_matches_from_scratch_over_subtrees` walks full legal subtrees from four positions chosen to exercise captures, castling both sides, en passant, and promotions with/without capture, asserting incremental == from-scratch after every make and every unmake over deep sequences. `make_then_unmake_restores_the_accumulator_exactly` covers exact restoration. `accumulator_of_a_clone_matches_a_fresh_computation` covers the clone case.

**Benchmark (AC#6).** Controlled round-robin on `benches/search.rs` (Apple M3 Pro), branch tip vs its own base 18a4fa2 — the pair differs only by this change. Node counts are byte-identical between the two (e.g. startpos depth 9 = 10,823,803 nodes both), confirming the incremental evaluation reproduces from-scratch exactly and does not perturb the search tree. Wall-clock over that 10.8M-node tree: base median 1.104-1.217s, branch median 1.113-1.119s across two passes; the confidence intervals overlap (one base pass was a load outlier). NPS is unchanged within measurement noise on a loaded machine — the expected result, since the tapered eval is cheap and the per-make delta bookkeeping is comparable. The change's value is the seam for NNUE (AC#7), not an immediate throughput win.

Note: master advanced from this branch's base 18a4fa2 to 62d3b1d during the session (TASK-64.2 history heuristic, TASK-67 merged). The benchmark was therefore taken against the pinned branch base, not the moving master tip, to isolate this change. Integration onto the current master tip is the merge step's concern.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 22:20
---
Implementation handoff
Branch: task-64.15-incremental-eval-seam
Worktree: /Users/seabo/seaborg-worktrees/task-64.15-incremental-eval-seam
Base: 18a4fa2326d825abcd654b9ef3d54dbedf0832b9
Implementation target: f103853
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (45 + 276 + 19 + 1 passed; 0 failed; 2 pre-existing ignored in engine)
- cargo bench --bench search (controlled round-robin vs base 18a4fa2): node counts byte-identical; NPS unchanged within noise
Known failures: none. The 2 ignored engine tests are pre-existing at base 18a4fa2, unrelated to this change.
Note: master advanced to 62d3b1d after this branch was cut from 18a4fa2 (TASK-64.2, TASK-67); benchmark isolated against the pinned base.
---
<!-- COMMENTS:END -->
