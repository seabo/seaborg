---
id: TASK-69.1
title: 'NNUE design contract: feature set, topology, quantization, file format, loss'
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-20 19:39'
updated_date: '2026-07-20 22:39'
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
- [x] #1 A decision document under docs/ specifies feature set, topology, the parameterizable dimensions, quantization scheme, file format layout with a version header, and the training target formulation
- [x] #2 The file format section defines a version header sufficient for a loader to reject unknown or mismatched architectures deterministically
- [x] #3 The document states the self-play purity boundary: what internal priors are permitted and what external inputs are forbidden
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

author: @claude
created: 2026-07-20 22:39
---
Review attempt: 1
Reviewed branch: task-69.1-nnue-design-contract
Reviewed implementation: 8f8d6a1f4c3df72567bbfce17c38aa33cdacc439
Verdict: approved

Immutability: target 8f8d6a1 descends from base 8c863f5; branch tip 0084bc4 adds only handoff metadata to the task file (no implementation-file change after the target). Base-to-target diff is docs-only: docs/nnue-design-contract.md (new) + the task file; no accidental scope.

Acceptance criteria:
- AC#1 (met): docs/nnue-design-contract.md specifies feature set, topology, parameterizable dimensions, quantization scheme, file-format layout with a version header, and the training-target formulation.
- AC#2 (met): the file-format section defines a fixed 64-byte SBNN version header plus 9 deterministic rejection rules (magic, format_version, feature_set_id/activation_id unimplemented, dimension mismatches, non-positive scales, non-zero reserved, param_bytes disagreement, param_hash mismatch) sufficient to reject unknown/mismatched architectures before interpreting weights.
- AC#3 (met): the self-play purity boundary is stated concretely — permitted internal priors (hand-crafted eval seed, design priors, hyperparameters, own search scores/outcomes) vs forbidden external inputs (external games/engines/books/human labels/tablebase labels).

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: exit 0, clean
- cargo test --workspace: 432 passed, 0 failed (45+300+68+19+1 doctest; 2 ignored)
- Repo-seam accuracy: all cited conventions verified against source (relative_square, PieceType ordinals, square indexing, PieceDeltaSink signature/location, EvalState/pst_index, Score bands, Search::evaluate).
- Internal consistency: feature-index range 0..768; quantization math coherent (eval_cp ≈ fout·SCALE with training target driving fout ≈ search_cp/SCALE); header is 64 bytes with all fields naturally aligned; param_bytes formula correct.
No benchmark run: diff touches no movegen/search hot path (docs-only).
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds docs/nnue-design-contract.md, the accepted decision record that TASK-69.2–.12 implement against. It fixes: the perspective-doubled 768-input feature set with a normative feature-index formula; the feature-transformer → clipped-ReLU → linear topology with parameterizable H/activation/scale; the int16/int32/i64 quantization scheme with round-half-away-from-zero dequantization; a versioned 64-byte SBNN header plus ordered parameter blob with deterministic rejection rules; and the blended sigmoid-space training target with a shared SCALE and scheduled lambda. It also states the self-play purity boundary (permitted internal priors vs forbidden external inputs). Docs-only; no Rust source touched. Verified: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings exit 0 (clean); cargo test --workspace 432 passed / 0 failed. All repo-seam facts the contract relies on were cross-checked against source (relative_square = sq ^ (player*56), PieceType Pawn=1..King=6, Square rank*8+file, PieceDeltaSink at chess/src/position/mod.rs:162, EvalState/pst_index/i16 tapered eval, Score bands ±10_000 cp / ±20_000 mate, Search::evaluate).
<!-- SECTION:FINAL_SUMMARY:END -->
