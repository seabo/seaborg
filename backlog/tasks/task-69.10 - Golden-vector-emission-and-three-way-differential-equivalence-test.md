---
id: TASK-69.10
title: Golden-vector emission and three-way differential equivalence test
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 15:44'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented golden-vector emission (Python) and a three-way cross-language differential test (Rust), plus a committed fixture that ties them together.

Design decisions:
- features_from_fen (export.py) is a second, FEN-based derivation of the perspective-768 feature indices, independent of the packed-record decoder data.decode uses. This makes the Python evaluation path fully independent of the engine, so the differential check exercises the feature encoding across languages, not just the arithmetic. A test cross-checks it against encode_record+data.decode on shared placements.
- _golden_network (export.py) is deterministic and patterned (no trained checkpoint needed for the committed fixture). Units 1..H-1 have varied weights (~[-516,516]) so distinct positions get distinct scores and the clip at QA is active; unit 0 is a uniform +900 so a maximally dense board drives its accumulator to ~0.88*i16 (28794, held in i16, no overflow) -- the near-overflow regime. |b_ft|+32*max|W_ft| stays inside i16, confirmed by the exporter's own _assert_accumulator_fits_i16.
- GOLDEN_POSITIONS spans tactical/endgame/king-safety/near-overflow; near-overflow = maximally dense boards (start, all-queens, all-rooks, 32 pieces). No golden score hits the +/-10000 clamp, so exact large-value arithmetic is checked, not just the clamp.
- Rust: forward was factored into forward + an inlined generic forward_with(dot). Production forward passes the runtime dispatcher dot_clipped_selected exactly as before (inlined, no perf change); the test passes dot_clipped and the AVX2 kernel explicitly so scalar and SIMD are compared through the identical tail. The differential test lives in the inference.rs #[cfg(test)] module because the explicit three-way needs the private dot_clipped/dot_clipped_avx2.
- SIMD arm is #[cfg(target_arch="x86_64")] + is_x86_feature_detected!("avx2"): a real three-way on x86_64 CI, compiled out on the aarch64 dev host (scalar vs Python only there). Verified the x86_64 arm compiles and passes strict clippy via cargo check/clippy --target x86_64-apple-darwin.
- CI (.github/workflows/ci.yml) does not run the Python suite; the committed golden_v1.{sbnn,vectors} is what the Rust gate enforces cross-language. test_export.py additionally guards that the committed fixture still matches the current exporter (fails if it goes stale), run locally.

Fixture regeneration: python export.py --emit-golden engine/tests/fixtures
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 15:28
---
Claiming for implementation on task-69.10-golden-vector-differential (worktree /Users/seabo/seaborg-worktrees/task-69.10-golden-vector-differential, base a5e52e6). Dependencies TASK-69.9 (export path + integer_eval_cp) and TASK-69.4 (scalar forward + golden harness) are Done. Approach: Python exporter emits a committed golden fixture (network .sbnn + (category,FEN,expected) vectors) computed by its integer forward; a Rust cfg(test) differential test asserts the scalar forward (always) and the AVX2 forward (x86-64 w/ AVX2) reproduce those exact integers, giving the three-way cross-language check.
---

author: @claude
created: 2026-07-21 15:44
---
Implementation handoff
Branch: task-69.10-golden-vector-differential
Worktree: /Users/seabo/seaborg-worktrees/task-69.10-golden-vector-differential
Base: a5e52e604b0db0d87346785b1052a9bd268ac937
Implementation target: 11e589398154e7ae899d93b955541b675abf0b6a
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (engine 388 passed incl. golden_vectors_agree_across_python_scalar_and_simd; loads_exported_network + build_metadata integration tests green)
- cargo check -p engine --tests --target x86_64-apple-darwin: pass (typechecks the AVX2 three-way arm compiled out on this aarch64 host)
- cargo clippy -p engine --all-targets --target x86_64-apple-darwin -- -D warnings: pass
- tools/trainer: .venv/bin/python -m unittest discover -p 'test_*.py': 51 passed (20 in test_export.py, incl. the new FeaturesFromFenTest and GoldenVectorTest)
Known failures: none. Note: on this aarch64 host the SIMD arm of the differential test is cfg-compiled out, so it runs scalar-vs-Python only; the three-way (scalar + AVX2 + Python) arm is exercised on x86_64 CI. CI does not run the Python suite; the committed engine/tests/fixtures/golden_v1.{sbnn,vectors} is what the Rust gate checks across languages.
---
<!-- COMMENTS:END -->
