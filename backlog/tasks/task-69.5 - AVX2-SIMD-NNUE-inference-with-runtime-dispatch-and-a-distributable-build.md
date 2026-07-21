---
id: TASK-69.5
title: AVX2 SIMD NNUE inference with runtime dispatch and a distributable build
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-21 03:54'
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
- [x] #1 The AVX2 forward pass is bit-identical to the scalar path over the golden vectors and a randomized position set
- [x] #2 The inference path is chosen at runtime by CPU feature detection and falls back to scalar when the wide path is unavailable
- [x] #3 The blanket target-cpu=native default is replaced by a distributable baseline plus runtime dispatch, and the workspace MSRV is declared
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the AVX2 NNUE inference path as a pure optimization of the scalar reference.

Design:
- Only the output-layer clipped dot product is dispatched. `forward` calls `dot_clipped_selected`, which uses `is_x86_feature_detected!("avx2")` on x86_64 and falls back to the scalar `dot_clipped`; on non-x86-64 targets only the scalar path is compiled. The bias seed, i64 SCALE multiply, round-half-away divide, and centipawn clamp remain shared, so the two paths can differ only in summation, not rounding.
- `dot_clipped_avx2` (`#[target_feature(enable="avx2")]`) clips activations with `_mm256_max_epi16(.,0)` then `_mm256_min_epi16(., min(qa, i16::MAX))`, multiply-accumulates 16 i16 lanes via `_mm256_madd_epi16` into an i32 vector, and horizontally reduces. The clip cap at i16::MAX is exact: activations are i16, so when qa>i16::MAX the upper clamp never binds. Bit-identity holds because integer addition is associative/commutative with no overflow, which the contract's bound on |s| guarantees.
- Hidden width is a multiple of 16, so the block is processed with full 256-bit loads and no scalar remainder.

Build story (AC#3):
- Replaced blanket `[build] rustflags = -C target-cpu=native` in .cargo/config.toml with a per-architecture distributable baseline: x86-64-v2 for x86_64 (keeps hardware POPCNT for movegen; deliberately below AVX2 so AVX2 stays the runtime-detected wider path), toolchain defaults elsewhere so aarch64 dev/builds are unaffected. Per-arch because [build] rustflags applies to every target and an x86-only target-cpu would break aarch64.
- Declared workspace MSRV rust-version=1.93 in [workspace.package], inherited by all members. Declaring it surfaced a real fact: the true floor is 1.93 (MaybeUninit::assume_init_ref/assume_init_mut in chess/src/movelist.rs), not the 1.87 the is_multiple_of use suggested. Clippy's incompatible_msrv lint now enforces it.
- Updated the now-stale ci.yml comment that described target-cpu=native.

Verification:
- cargo fmt --check: clean.
- cargo clippy --workspace --all-targets --all-features -D warnings: clean on native aarch64 AND cross-target x86_64-apple-darwin (which compiles and lints the AVX2 code path; incompatible_msrv enforced against 1.93).
- cargo test --workspace (native aarch64): all pass (AVX2 tests cfg-compiled out on this arch).
- cargo test --target x86_64-apple-darwin -p engine: all pass; the two AVX2 differential tests SKIP under Rosetta because it does not expose AVX2 via CPUID (verified via --nocapture skip message), confirming the graceful-fallback branch.

AVX2 execution caveat: the AVX2 kernel cannot be executed on this aarch64 host (no x86 hardware; Rosetta lacks AVX2). It is verified locally by x86_64 compilation and manual review of the intrinsic selection and horizontal-sum shuffle; it is executed for real by the differential tests on CI (ubuntu-latest x86_64, which exposes AVX2). Reviewers on x86_64 AVX2 hardware will run the differential tests directly.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 03:27
---
Implementation handoff
Branch: task-69.5-avx2-simd-nnue-inference
Worktree: /Users/seabo/seaborg-worktrees/task-69.5-avx2-simd-nnue-inference
Base: 05880a59a02a47f388fafad164e482fb764c7ccc
Implementation target: 1f54c227f022c9bb46c5375f61212a351af66dbb
Resolved findings: none
Verification:
- cargo fmt --check: pass (native aarch64)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (native aarch64 and cross-target x86_64-apple-darwin, so the AVX2 code and the 1.93 MSRV lint are both exercised)
- cargo test --workspace: pass (native aarch64; AVX2 differential tests cfg-compiled out here)
- cargo test --target x86_64-apple-darwin -p engine: pass; AVX2 differential tests skip gracefully under Rosetta (no AVX2 via CPUID) — they execute on x86_64 AVX2 hardware (CI ubuntu-latest)
Known failures: none. Note: the AVX2 kernel itself cannot be executed on this aarch64 host; it is verified locally by x86_64 compilation + manual intrinsic review and is executed by the differential tests on CI. A reviewer on x86_64 AVX2 hardware can run `cargo test -p engine nnue::inference::tests::avx2` to exercise it directly.
---

author: @claude
created: 2026-07-21 03:54
---
Review attempt: 1
Reviewed branch: task-69.5-avx2-simd-nnue-inference
Reviewed implementation: 1f54c227f022c9bb46c5375f61212a351af66dbb
Base: 05880a59a02a47f388fafad164e482fb764c7ccc
Verdict: approved

Immutability/scope: target descends from base; the only commit after the target (93e8b59) touches solely the task file; the full base-to-target diff is on-topic (AVX2 kernel + dispatch + differential tests in inference.rs, .cargo/config.toml + ci.yml comment, MSRV declaration). No #[allow] introduced.

Kernel review: clip via _mm256_max_epi16(.,0) then _mm256_min_epi16(., min(qa,i16::MAX)) reproduces scalar clamp(0,qa) for every reachable i16 activation; _mm256_madd_epi16 over non-negative clipped activations cannot overflow a madd pair; the horizontal reduction folds all 8 i32 lanes correctly; bit-identity holds because i32 addition is associative/commutative within the contract's no-overflow regime. SAFETY comments on the target_feature fn and the runtime-gated call are accurate.

AC#1 (bit-identical AVX2 forward over golden + randomized positions): PROVEN by execution, not code inspection. The review host is aarch64 and neither native, Rosetta (x86_64-apple-darwin), nor Rosetta-backed Docker amd64 exposes AVX2, so the differential tests skip there. I therefore executed them under genuine AVX2: an amd64 rust:slim container on QEMU (Docker Rosetta emulation disabled) where is_x86_feature_detected!("avx2")==true (rustc 1.97.1). Both tests ran (no skip) and passed:
  test nnue::inference::tests::avx2_dot_product_is_bit_identical_to_the_scalar_oracle ... ok
  test nnue::inference::tests::avx2_forward_matches_the_scalar_path_over_golden_and_random_positions ... ok
Command: docker run --platform linux/amd64 -v <worktree>:/w:ro -w /w -e CARGO_TARGET_DIR=/tmp/t rust:slim cargo test --locked -p engine --lib nnue::inference::tests::avx2 -- --nocapture

AC#2 (runtime detection + scalar fallback): PROVEN via both branches. AVX2 path executes when detected (container run above); scalar fallback executes when AVX2 is absent (cargo test --target x86_64-apple-darwin -p engine under Rosetta: 375 pass, the two AVX2 tests skip gracefully with the documented note).

AC#3 (distributable baseline + MSRV): PROVEN. .cargo/config.toml replaces target-cpu=native with per-arch x86-64-v2 (retains POPCNT, stays below AVX2 so AVX2 remains the runtime-detected path) and toolchain defaults elsewhere; rust-version=1.93 declared in [workspace.package] and inherited by all members. A clean cross-target clippy (fresh CARGO_TARGET_DIR, x86_64-apple-darwin) compiles the AVX2 code and enforces incompatible_msrv against 1.93.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings (native aarch64): pass
- cargo clippy ... --target x86_64-apple-darwin (clean CARGO_TARGET_DIR): pass — compiles AVX2 code, enforces MSRV 1.93 lint
- cargo test --workspace (native aarch64): pass
- cargo test --target x86_64-apple-darwin -p engine (Rosetta): 375 pass; AVX2 tests skip gracefully (no AVX2)
- AVX2 differential tests under real AVX2 (QEMU amd64 container, rustc 1.97.1, --locked): 2 passed, 0 skipped

Benchmarks: not run. The eval-path change is a #[inline] dispatch that cfg-compiles to the original scalar call on this aarch64 host (identical codegen); the x86-64-v2 baseline change applies only to x86 builds (unmeasurable here) and deliberately retains hardware POPCNT for movegen. No observable hot-path delta exists on the review hardware.

No blocking findings. Approving at 1f54c22.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a hand-written AVX2 clipped-dot-product kernel for NNUE inference, dispatched at runtime by is_x86_feature_detected!("avx2") with the scalar reference as fallback, plus a distributable build story (per-arch x86-64-v2 baseline replacing target-cpu=native) and a declared workspace MSRV (rust-version=1.93). Reviewed at implementation SHA 1f54c22. AC#1 proven by executing the two differential tests under genuine AVX2 (amd64 container on QEMU, avx2 detected, rustc 1.97.1, --locked): both passed with no skip, confirming the AVX2 forward pass is bit-identical to the scalar oracle over the golden vectors and randomized positions. AC#2 proven by both dispatch branches: the AVX2 path executes when detected (container run) and the scalar path runs when AVX2 is absent (x86_64-apple-darwin under Rosetta: 375 engine tests pass, AVX2 tests skip gracefully). AC#3 proven by the .cargo/config.toml/ci.yml diff and MSRV declaration, with a clean cross-target clippy enforcing incompatible_msrv against 1.93. fmt/clippy/test all clean.
<!-- SECTION:FINAL_SUMMARY:END -->
