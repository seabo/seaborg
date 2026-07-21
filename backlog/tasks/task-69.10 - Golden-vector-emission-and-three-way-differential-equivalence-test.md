---
id: TASK-69.10
title: Golden-vector emission and three-way differential equivalence test
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 15:28'
labels:
  - nnue
  - training
  - inference
dependencies:
  - TASK-69.9
  - TASK-69.4
parent_task_id: TASK-69
priority: high
ordinal: 112000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Close the Rust/Python sync loop. The export step (TASK-69.9) also emits a golden-vector set of (FEN, expected-score) pairs spanning tactical, endgame, king-safety, and near-overflow positions. A test asserts three-way equality on that set: the PyTorch quantized forward pass, the Rust scalar forward pass (TASK-69.4), and — when present — the Rust SIMD forward pass (TASK-69.5) all produce the same integer scores.

This is the entire cross-language sync guarantee. With it, keeping the two implementations in step is a solved problem; without it, drift is a recurring, hard-to-localize source of lost strength.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Export emits a golden-vector set covering tactical, endgame, king-safety, and near-overflow positions
- [ ] #2 A differential test asserts PyTorch quantized forward equals the Rust scalar forward exactly over the golden vectors
- [ ] #3 When the SIMD path is available it is included in the equality assertion, giving a three-way check
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Python (export.py): add features_from_fen(fen) deriving the perspective-768 (stm, nstm) active-feature indices from a FEN via the contract formula (independent of the packed-record decode path, cross-checked against it in tests). Add a deterministic golden network (_golden_network) with weights large enough to drive dense positions into the wide integer range and keep the clip active, within the export overflow guards. Add GOLDEN_POSITIONS: curated (category, FEN) list covering tactical, endgame, king-safety, and near-overflow (max-material/dense) categories. Add golden_vectors(net, positions) computing integer_eval_cp per FEN, and write_golden_fixture emitting the .sbnn network plus a <category>TAB<FEN>TAB<cp> vectors text file. Wire a --emit-golden DIR CLI (and optional --golden alongside a real checkpoint export).
2. Commit the emitted fixtures under engine/tests/fixtures/golden_v1.sbnn and golden_v1.vectors.
3. Rust (inference.rs): refactor forward into forward + inlined generic forward_with(dot_fn) so tests can force the scalar and the AVX2 dot paths through the identical tail; production forward unchanged (inlines dot_clipped_selected). Add a #[cfg(test)] differential test that loads the committed golden fixture, and for every (category, FEN, expected) asserts the Rust scalar forward equals Python's emitted expected, and — on x86-64 with AVX2 — the SIMD forward too (three-way). Assert all four categories are present.
4. Python tests (test_export.py): features_from_fen agrees with the encode_record+data.decode path on shared positions; golden set covers all four categories and near-overflow positions reach the widest accumulator magnitudes; integer_eval_cp reproduces the committed expected values (fixture self-consistency); golden fixture round-trips through QuantizedNetwork.from_bytes.
5. Run cargo fmt --check, clippy -D warnings, cargo test --workspace, and the trainer unittest suite; hand off for review.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 15:28
---
Claiming for implementation on task-69.10-golden-vector-differential (worktree /Users/seabo/seaborg-worktrees/task-69.10-golden-vector-differential, base a5e52e6). Dependencies TASK-69.9 (export path + integer_eval_cp) and TASK-69.4 (scalar forward + golden harness) are Done. Approach: Python exporter emits a committed golden fixture (network .sbnn + (category,FEN,expected) vectors) computed by its integer forward; a Rust cfg(test) differential test asserts the scalar forward (always) and the AVX2 forward (x86-64 w/ AVX2) reproduce those exact integers, giving the three-way cross-language check.
---
<!-- COMMENTS:END -->
