---
id: TASK-69.1
title: 'NNUE design contract: feature set, topology, quantization, file format, loss'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:39'
updated_date: '2026-07-20 21:26'
labels:
  - nnue
  - design
dependencies: []
parent_task_id: TASK-69
priority: high
ordinal: 103000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Produce the single shared contract every other NNUE subtask forks from, recorded as a decision document under docs/. Fix the decisions that are expensive to change once implementation fans out, and deliberately leave parameterizable what is cheap to vary.

Must decide and document: the input feature set (recommended starting point: perspective-doubled piece-square, 768x2 inputs, no king buckets, because incremental update is trivial and it proves the whole pipeline end to end before a costlier HalfKA-style set); network topology and the set of dimensions that stay parameterizable (hidden width, activation, output scaling); the quantization scheme (integer types, scale factors, clipped-activation semantics, saturation/overflow behaviour) since this is where the Rust and PyTorch paths most often silently diverge; the on-disk file format (a versioned header carrying architecture parameters and quantization scales, such that a loader refuses a file it does not understand rather than misinterpreting it); the training target formulation (blend of search score and game WDL outcome with a lambda, and how lambda is scheduled); and the self-play purity boundary in concrete terms.

This subtask is a decision record, not code. It is the contract subtasks .2 through .12 implement against.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A decision document under docs/ specifies feature set, topology, the parameterizable dimensions, quantization scheme, file format layout with a version header, and the training target formulation
- [ ] #2 The file format section defines a version header sufficient for a loader to reject unknown or mismatched architectures deterministically
- [ ] #3 The document states the self-play purity boundary: what internal priors are permitted and what external inputs are forbidden
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Research existing eval infrastructure: tapered eval score type/units, EvalState + eval_stack, PieceDeltaSink trait, board representation (square/piece/color indexing), workspace crate layout, docs conventions.
2. Author docs/nnue-design-contract.md as a decision record covering: (a) input feature set — perspective-doubled 768x2 piece-square, no king buckets, with rationale; (b) network topology and the parameterizable dimensions (hidden width, activation, output scaling); (c) quantization scheme — integer types, scale factors, clipped-activation semantics, saturation/overflow, tying Rust and PyTorch paths; (d) on-disk file format — versioned header carrying architecture params + quant scales, deterministic rejection of unknown/mismatched files; (e) training target — blended search-score/game-WDL with a scheduled lambda; (f) self-play purity boundary in concrete terms (permitted internal priors, forbidden external inputs).
3. Cross-check the contract against the real repo seam (square/piece/color conventions, eval units, PieceDeltaSink signature) so subtasks .2-.12 implement against accurate facts.
4. Verify each acceptance criterion against the document; run repo-required checks (no code change, but confirm no regressions); hand off for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Authored docs/nnue-design-contract.md as the shared NNUE contract. Decisions were cross-checked against the live repo seam before writing: square indexing a1=0 rank-major (chess/src/position/square.rs), Piece/PieceType/Player enums and Player::relative_square (chess/src/position), the PieceDeltaSink trait signature (chess/src/position/mod.rs:162), EvalState/pst_index and the i16-centipawn tapered eval (engine/src/eval.rs), and the Score centipawn band +/-10_000 with mate at +/-20_000 (engine/src/score.rs). The feature-index formula, i16 accumulator, and centipawn clamp are all stated in those terms so subtasks .2-.12 implement against accurate conventions. Docs-only change; no Rust source touched.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 21:26
---
Implementation handoff
Branch: task-69.1-nnue-design-contract
Worktree: /Users/seabo/seaborg-worktrees/task-69.1-nnue-design-contract
Base: 8c863f5e6ed50f9914cf7f7c66beda73582bc4da
Implementation target: 8f8d6a1f4c3df72567bbfce17c38aa33cdacc439
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (432 tests: 45 + 300 + 68 + 19 + 1 doctest; 2 ignored; 0 failed)
Known failures: none
---
<!-- COMMENTS:END -->
