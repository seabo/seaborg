---
id: TASK-69.3
title: NNUE feature encoding and accumulator as a PieceDeltaSink
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-20 23:23'
labels:
  - nnue
  - inference
dependencies:
  - TASK-69.1
parent_task_id: TASK-69
priority: high
ordinal: 105000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the input feature indexing from the design contract and the network accumulator that maintains the first-layer activations incrementally, as a new PieceDeltaSink consumer alongside EvalState. This is the core engine integration and the one place a subtle bug would silently cost strength, so it is scoped tightly and validated exactly like the existing incremental evaluation.

The accumulator plugs into the existing seam: Position::replay_last_move_deltas drives add/remove calls, the accumulator is threaded through Search with a push/pop stack for O(1) restore on unmake, and debug builds assert the incremental accumulator against a from-scratch recomputation at every node, reusing the validation pattern already established for EvalState. No forward pass or scoring yet; this task delivers a correct, incrementally-maintained accumulator and its equivalence guarantee.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Feature indices for both perspectives match the design contract and are covered by tests over representative positions
- [ ] #2 The accumulator is maintained incrementally across make and unmake and a debug assertion checks it against a from-scratch recomputation at every node
- [ ] #3 A make-then-unmake restores the accumulator bit-for-bit, and a subtree walk asserts incremental equals from-scratch, mirroring the existing EvalState tests
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine/src/nnue module (sibling of eval), declared in lib.rs.
2. Feature encoding: INPUT_DIM=768, feature_index(perspective, piece, square) = relative_square ^ + 64*pt0 + 384*side, per the design contract. Unit tests over representative pieces/squares/perspectives (both colours, friendly/enemy, orientation flip).
3. FeatureTransformer: in-memory i16 weight table (input_dim x H feature-major) + i16 bias, parameterizable H (multiple of 16). Minimal container the accumulator needs; the file loader (TASK-69.2) will construct it later.
4. Accumulator: two per-perspective i16 activation vectors seeded from bias; implements PieceDeltaSink (add/remove toggle one feature column per perspective). from_position rebuild is the from-scratch reference, mirroring EvalState::from_position.
5. Tests mirroring EvalState: subtree walk asserting incremental == from-scratch at every node (make and unmake) across captures/castling/en-passant/promotions; make-then-unmake bit-for-bit restore; clone equivalence. Use a deterministic synthetic FeatureTransformer with bounded weights.
6. Run cargo fmt --check, clippy -D warnings, cargo test --workspace. Hand off for review. No forward pass/scoring and no Search wiring (deferred to TASK-69.4).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the NNUE feature encoding and accumulator in a new engine/src/nnue.rs module (sibling of eval, per the design contract), registered as pub mod nnue in engine/src/lib.rs.

- feature_index(perspective, piece, square) implements the contract's normative formula: oriented = perspective.relative_square(square).index(); piece_type_0 = PieceType - 1 (Pawn=0..King=5); side = 0 (friendly) or 384 (enemy); index = oriented + 64*piece_type_0 + side, in 0..768. Reuses relative_square and PieceType ordinals exactly as the contract specifies.
- FeatureTransformer: in-memory i16 first-layer weights (INPUT_DIM x H, feature-major so a feature's H-column is contiguous, matching the on-disk blob layout) plus i16 bias. Constructor enforces H a positive multiple of 16 and correct lengths. Kept minimal to what the accumulator needs; the TASK-69.2 loader will construct one from a file.
- Accumulator: two per-perspective i16 activation vectors (White slot 0, Black slot 1), seeded to bias. Implements PieceDeltaSink: add/remove fold one weight column into each perspective. from_position rebuilds from scratch and is the equivalence reference, driving the same add path (single source of the per-piece arithmetic), mirroring EvalState::from_position. PartialEq compares activations only; Debug prints the two vectors.

Scope: no forward pass, scoring, or Search wiring in this task (deferred to TASK-69.4, which owns the Search::evaluate selection seam per the design contract). The at-every-node incremental-vs-from-scratch guarantee is validated by the subtree-walk test, the pattern EvalState uses.

Design note: activations use i16 arithmetic (plain +=/-=), so a debug build panics on overflow rather than silently wrapping, matching the contract's 'i16 overflow is a defect, not a wrap'. Both incremental and from-scratch paths use identical commutative/associative integer addition, so they agree bit-for-bit. Test weights are bounded (|w|<=7, |bias|<=2) so the <=32 active features per perspective stay far inside i16.

Note for the reviewer/integrator: TASK-69.2 (file format/loader) is being implemented in parallel and will also touch the nnue module; a merge-time collision on the module is possible and is an integration concern, not a defect here.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 22:58
---
Claiming for implementation on task-69.3-nnue-accumulator.
---

author: @claude
created: 2026-07-20 23:23
---
Implementation handoff
Branch: task-69.3-nnue-accumulator
Worktree: /Users/seabo/seaborg-worktrees/task-69.3-nnue-accumulator
Base: 6d3d4ac98a40a455959b4cea18d0b0a82b0c7867
Implementation target: 9d2096a5f5b8f418a760f9b76af636ebb2e4bd98
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean, exit 0)
- cargo test --workspace: pass (engine 308 passed / 0 failed / 2 ignored, incl. 5 new nnue tests; workspace suites all green)
Known failures: none
---
<!-- COMMENTS:END -->
