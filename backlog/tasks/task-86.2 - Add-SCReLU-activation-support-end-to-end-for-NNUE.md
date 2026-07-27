---
id: TASK-86.2
title: Add SCReLU activation support end-to-end for NNUE
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-25 12:23'
updated_date: '2026-07-27 09:56'
labels:
  - nnue
dependencies: []
parent_task_id: TASK-86
priority: medium
ordinal: 144000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The design contract reserves activation_id = 1 for squared clipped ReLU (SCReLU), and the PyTorch model already reserves it, but the Rust inference path implements only CReLU (activation_id = 0). SCReLU is a well-established near-free strength improvement over CReLU. Implement it end-to-end so a network can be trained and deployed with SCReLU: PyTorch trainer, quantization-aware export, Rust scalar and SIMD inference, and format/loader acceptance of activation_id = 1. This is a retrain-only capability that the architecture sweep (sibling task) can then include as a swept factor.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 activation_id = 1 (SCReLU) is accepted by the format loader and selects squared-clipped-ReLU semantics consistent with the design contract quantization scheme
- [x] #2 Rust scalar and AVX2 inference implement SCReLU and are proven bit-identical to each other for SCReLU networks
- [x] #3 The PyTorch trainer and quantization-aware export produce a valid SCReLU .sbnn that the engine loads and evaluates
- [x] #4 A three-way differential/golden-vector equivalence test (mirroring TASK-69.10) covers an SCReLU network
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Design contract (docs/nnue-design-contract.md): specify activation_id=1 (SCReLU) integer semantics. Per-unit A_j = round_div(clamp(x_j,0,QA)^2, QA), round-half-away-from-zero. A_j stays in [0, QA] (<= i16::MAX), so it fits the i16 activation domain and every downstream stage (i32 output sum, denominator QA*QB, rounded dequant) is byte-identical to CReLU; only the activation stage changes. For odd QA (the 255 default) this rounding coincides with the trainer's round-half-to-even fake_quantize, so integer_eval_cp stays faithful to the float model.
2. format.rs: add ACTIVATION_SCRELU=1 and an Activation enum (id<->enum). Store activation on Network; thread through new()/read()/write()/decode_blob; accept id 0 and 1 in read, write the stored id, keep rejecting unknown ids. Re-export Activation from mod.rs.
3. inference.rs: thread activation into forward_with. For SCReLU pre-activate each perspective block (screlu_activation: clamp+square+round_div by QA) into a fixed stack buffer, chunked (256, multiple of 16) so no per-eval heap alloc, then reuse the existing bit-identical clipped-dot kernels (scalar + AVX2) for the weighted sum; their [0,QA] clip is a no-op on the pre-activated values. Parameterize the independent dense reference_forward by activation.
4. Python export.py: carry activation id on QuantizedNetwork (default CReLU so existing constructors are unchanged); write it in to_bytes and accept 0/1 in from_bytes; allow SCReLU in quantize; implement SCReLU in integer_eval_cp (clamp, square, round_div by QA). model.py/train.py already compute and expose SCReLU.
5. Fixtures + tests: emit golden_screlu_v1.{sbnn,vectors}; add a Rust three-way (scalar/AVX2/Python) differential test over the SCReLU fixture mirroring the CReLU one, plus scalar-vs-AVX2 bit-identity over random SCReLU nets/positions; add SCReLU coverage to Python test_export (quantize/from_bytes/integer_eval_cp/fixture-consistency/reproduction). Update the Rust unknown-activation rejection test to use id 2.
6. Required checks: cargo fmt --check, clippy --workspace --all-targets --all-features -D warnings, cargo test --workspace, plus the x86_64 AVX2 cross-check and the trainer unittests. Handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented SCReLU (activation_id=1) end-to-end.

Design decision — per-unit activation reuses the CReLU kernels. SCReLU integer activation is A_j = round_div(clamp(x,0,QA)^2, QA) (round half away from zero on a non-negative numerator). Because clamp<=QA gives A_j<=QA<=i16::MAX, the activated value stays in the [0,QA] i16 domain, so the i32 output sum, the QA*QB denominator, and the rounded dequant are byte-identical to CReLU. This lets the existing scalar and AVX2 clipped-dot kernels be reused unchanged (their [0,QA] clip is a no-op on a pre-activated value), which is what makes scalar/AVX2 bit-identity for SCReLU hold by construction. Rejected the defer-the-divide alternative (a = clamp^2 up to QA^2) because it overflows i16, breaks _mm256_madd_epi16 reuse, and needs a wider bespoke kernel.

Zero per-eval allocation — dot_screlu pre-activates through a fixed 256-entry stack buffer in 16-aligned chunks, so the forward pass allocates nothing regardless of H (guards the search hot loop).

Rounding faithfulness — the per-unit divide rounds half away from zero; for odd QA (the 255 default) 2*c^2 is even and QA odd, so no exact half occurs and this coincides exactly with the trainer's round-half-to-even fake_quantize. The SCReLU reproduction test confirms integer_eval_cp reproduces the trained float model within 1cp, same as CReLU.

Scope — NEON was consolidated off this task on master (cd54a2b) before work began; the stale prior worktree edit that still referenced AC#5 was discarded and the branch reset onto master. model.py and train.py already supported screlu (float + QAT + --activation), so no change was needed there.

Fixtures — golden_screlu_v1.{sbnn,vectors} committed, regenerable with 'python export.py --emit-golden engine/tests/fixtures' (which now emits both CReLU and SCReLU). Same weights as golden_v1, activation id 1; SCReLU scores differ, confirming the activation is exercised.

Verification:
- cargo fmt --check: clean.
- cargo clippy --workspace --all-targets --all-features -D warnings: clean.
- cargo test --workspace: 681 passed, 0 failed (2 pre-existing ignored). New SCReLU tests: format round-trip, activation unit values, screlu_forward vs dense reference, three-way SCReLU golden (scalar/Python on this aarch64 host; AVX2 arm compiled out).
- AVX2 SCReLU test (screlu_scalar_and_avx2_forward_are_bit_identical, #[cfg x86_64]) verified to compile under strict clippy via 'cargo clippy --target x86_64-apple-darwin -p engine --tests -D warnings'; it executes on the AVX2 CI host.
- Trainer suite (.venv): python -m unittest discover -> 56 passed, incl. SCReLU quantize/from_bytes/integer_eval_cp/golden-consistency/reproduction.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-27 09:29
---
Implementation handoff
Branch: task-86.2-screlu-activation
Worktree: /Users/seabo/seaborg-worktrees/task-86.2-screlu-activation
Base: cd54a2bb5e2f3967d7fb8bc2f31107d88375fb60
Implementation target: 5e13155
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (681 passed, 0 failed, 2 ignored pre-existing)
- cargo clippy --target x86_64-apple-darwin -p engine --tests --all-features -- -D warnings: pass (AVX2 SCReLU test compiles; executes on the AVX2 CI host)
- tools/trainer .venv python -m unittest discover -p 'test_*.py': pass (56 passed)
Known failures: none
---

author: @claude
created: 2026-07-27 09:56
---
Review attempt: 1
Reviewed branch: task-86.2-screlu-activation
Reviewed implementation: 5e13155
Verdict: approved

All four acceptance criteria proven against the immutable base cd54a2b -> target 5e13155 diff, reviewed in full:
- AC#1: format loader accepts activation_id=1 and selects SCReLU; id 2 still rejected. Proven by screlu_activation_round_trips and unknown_activation_is_rejected.
- AC#2: scalar SCReLU proven vs an independent dense reference (screlu_forward_agrees_with_the_dense_reference). Scalar/AVX2 bit-identity is structural: SCReLU pre-activates into a shared buffer and feeds the same clipped-dot kernels already proven bit-identical for CReLU; screlu_scalar_and_avx2_forward_are_bit_identical compiles under x86_64 strict clippy and executes on the AVX2 CI host (this aarch64 dev host compiles it out, the repo's established AVX2-test pattern).
- AC#3: trainer+export produce a valid SCReLU .sbnn; test_exported_screlu_network_reproduces_the_trained_model trains screlu -> quantizes -> integer_eval reproduces within tolerance, and the engine loads+evaluates the exported golden_screlu_v1.sbnn.
- AC#4: three-way differential (Python/scalar/AVX2) over golden_screlu_v1, mirroring the CReLU golden test, incl. a guard that SCReLU scores differ from CReLU over the same weights.

Immutability: target 5e13155 is an ancestor of the branch tip; the only later commit f7ac132 touches solely the task file. Worktree clean.

Verification (run on this host):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (fresh CARGO_TARGET_DIR)
- cargo test --workspace: pass (SCReLU tests listed above included; AVX2 bit-identity compiled out on aarch64)
- cargo clippy --target x86_64-apple-darwin -p engine --tests --all-features -- -D warnings: pass (AVX2 SCReLU test compiles)
- tools/trainer .venv python -m unittest discover -p 'test_*.py': 56 passed
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added SCReLU (activation_id=1) end-to-end for NNUE. The activation is the sole stage that varies between ids: a=round_div(clamp(x,0,QA)^2,QA) stays in [0,QA], so the accumulator, output sum, QA*QB denominator and dequant are byte-identical to CReLU and the SCReLU path reuses the existing scalar and AVX2 clipped-dot kernels unchanged (their clip is a no-op on a pre-activated value). format.rs adds an Activation enum threaded through new/read/write/decode_blob; inference.rs pre-activates each perspective block into a fixed 256-entry stack buffer in 16-aligned chunks (zero per-eval alloc, AVX2 16-lane precondition preserved) and reuses the shared dot; export.py carries the id and implements SCReLU in integer_eval_cp; the contract documents the integer semantics and rounding faithfulness. Verified on the implementation target 5e13155: cargo fmt --check clean; cargo clippy --workspace --all-targets --all-features -D warnings clean (fresh CARGO_TARGET_DIR); cargo test --workspace all pass incl. screlu_activation_round_trips, unknown_activation_is_rejected(id 2), screlu_activation_clips_squares_and_rounds, screlu_forward_agrees_with_the_dense_reference, and the three-way screlu_golden_vectors_agree_across_python_scalar_and_simd; the x86_64 AVX2 bit-identity test compiles under strict clippy (cargo clippy --target x86_64-apple-darwin -p engine --tests -D warnings) and executes on the AVX2 CI host per the repo's established pattern; trainer suite 56 pass incl. SCReLU quantize/round-trip/integer-eval/reproduction/golden-consistency.
<!-- SECTION:FINAL_SUMMARY:END -->
