---
id: TASK-84
title: Add a NEON NNUE inference kernel with runtime dispatch for aarch64
status: To Do
assignee: []
created_date: '2026-07-25 12:21'
labels: []
dependencies:
  - TASK-83
references:
  - engine/src/nnue/inference.rs
  - docs/nnue-design-contract.md
ordinal: 140000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The only hand-written SIMD kernel in the engine — the NNUE output-layer clipped dot product — is x86-only. The runtime dispatcher at `engine/src/nnue/inference.rs:121-134` selects the AVX2 kernel (`dot_clipped_avx2`, `inference.rs:177-227`, using `std::arch::x86_64` i16 intrinsics such as `_mm256_madd_epi16`) via `is_x86_feature_detected!("avx2")`; on aarch64 the whole `#[cfg(target_arch = "x86_64")]` block compiles out and Apple Silicon always falls back to the scalar loop (`dot_clipped`, `inference.rs:142-148`). Add a NEON kernel for aarch64 that mirrors the AVX2 kernel semantics, selected via `is_aarch64_feature_detected!` runtime dispatch, matching the pattern established by TASK-69.5. Preserve exact numerical equivalence with the scalar reference (this path is covered by the golden-vector / three-way differential test from TASK-69.10); where the quantization is int8, prefer FEAT_DotProd (`sdot`/`udot`) and note any i8mm opportunity. Update the NNUE design contract, which currently assumes an x86-64/AVX2-only SIMD path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A NEON aarch64 kernel computes the NNUE output-layer clipped dot product and is selected at runtime via `is_aarch64_feature_detected!`, with a safe scalar fallback when the feature is absent
- [ ] #2 The NEON kernel is numerically identical to the scalar reference across the existing golden-vector / differential equivalence tests, and those tests exercise the aarch64 path
- [ ] #3 A measured M3 before/after comparison (scalar-fallback vs NEON) is recorded using controlled same-machine measurement; no cross-session comparison
- [ ] #4 The NNUE design contract is updated to describe the aarch64/NEON path alongside the AVX2 path
<!-- AC:END -->
