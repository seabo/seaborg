---
id: TASK-69.5
title: AVX2 SIMD NNUE inference with runtime dispatch and a distributable build
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-21 03:11'
labels:
  - nnue
  - inference
  - simd
  - build
dependencies:
  - TASK-69.4
parent_task_id: TASK-69
priority: medium
ordinal: 107000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a hand-written AVX2 inference path as a pure optimization of the scalar reference (TASK-69.4), selected at runtime via feature detection with the scalar path as fallback, and give the workspace a distributable build story. Today .cargo/config.toml sets target-cpu=native with no runtime dispatch, so any SIMD would be silently machine-specific and non-distributable; replace that with an explicit baseline plus runtime detection of the wider path. Declare the workspace MSRV as part of this build-story work.

A differential test asserts the SIMD path is bit-identical to the scalar path over the golden vectors and randomized positions. Correctness is defined by equality with the scalar oracle, never re-derived.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The AVX2 forward pass is bit-identical to the scalar path over the golden vectors and a randomized position set
- [ ] #2 The inference path is chosen at runtime by CPU feature detection and falls back to scalar when the wide path is unavailable
- [ ] #3 The blanket target-cpu=native default is replaced by a distributable baseline plus runtime dispatch, and the workspace MSRV is declared
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. AVX2 kernel in engine/src/nnue/inference.rs: add a #[cfg(target_arch="x86_64")] #[target_feature(enable="avx2")] unsafe fn dot_clipped_avx2 that reproduces scalar dot_clipped exactly — max/min i16 clip to [0, min(qa,32767)], _mm256_madd_epi16 into i32 lanes, horizontal-sum. H is a multiple of 16 so no remainder loop.
2. Runtime dispatch: forward() calls a dot_clipped_selected helper that uses is_x86_feature_detected!("avx2") on x86_64 and falls back to scalar dot_clipped otherwise / on non-x86 targets. Shared i64 scale/round_div/clamp tail stays identical, so only the dot product differs.
3. Differential tests (x86_64+AVX2, graceful skip otherwise): assert dot_clipped_avx2 == dot_clipped over (a) randomized bounded activation/weight vectors incl. negatives and large qa, and (b) accumulators from randomized legal-move positions with contract-valid random networks; assert full forward() == independent dense reference_forward over the golden FENs and random positions. Existing golden-vector test now dispatches through AVX2 on x86_64.
4. Build story: replace blanket [build] target-cpu=native in .cargo/config.toml with a per-arch distributable baseline — x86-64-v2 for x86_64 (keeps hardware popcnt, leaves AVX2 as the runtime-detected wider path); default baseline elsewhere so aarch64 dev/build is unaffected. Update the now-stale CI comment referencing native.
5. Declare workspace MSRV: rust-version in [workspace.package], inherited by members. Floor is 1.87 (is_multiple_of, already used in format.rs).
6. Run fmt/clippy/test; note the AVX2 differential test is exercised on x86_64 CI, cfg-compiled out on the aarch64 handoff host.
<!-- SECTION:PLAN:END -->
