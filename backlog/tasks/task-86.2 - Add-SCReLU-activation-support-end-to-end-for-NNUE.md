---
id: TASK-86.2
title: Add SCReLU activation support end-to-end for NNUE
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-25 12:23'
updated_date: '2026-07-27 09:10'
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
- [ ] #1 activation_id = 1 (SCReLU) is accepted by the format loader and selects squared-clipped-ReLU semantics consistent with the design contract quantization scheme
- [ ] #2 Rust scalar and AVX2 inference implement SCReLU and are proven bit-identical to each other for SCReLU networks
- [ ] #3 The PyTorch trainer and quantization-aware export produce a valid SCReLU .sbnn that the engine loads and evaluates
- [ ] #4 A three-way differential/golden-vector equivalence test (mirroring TASK-69.10) covers an SCReLU network
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
