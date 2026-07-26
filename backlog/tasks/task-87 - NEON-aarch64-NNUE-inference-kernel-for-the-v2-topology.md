---
id: TASK-87
title: NEON aarch64 NNUE inference kernel for the v2 topology
status: To Do
assignee: []
created_date: '2026-07-26 23:09'
labels:
  - nnue
  - simd
  - aarch64
  - performance
dependencies:
  - TASK-83
  - TASK-86.2
  - TASK-86.4
references:
  - engine/src/nnue/inference.rs
  - docs/nnue-design-contract.md
priority: low
type: feature
ordinal: 149000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build the aarch64 NEON SIMD inference path ONCE, against the final v2 network topology. Supersedes the archived TASK-84, which was archived because its kernel targeted the pre-v2 single-layer network that TASK-86.4 replaces. Consolidates the NEON parity that was previously scattered as AC#5 on TASK-86.2 (SCReLU) and TASK-86.4 (topology v2); those ACs were removed so the critical-path eval work ships on scalar + AVX2 without each feature task standing up SIMD-kernel infrastructure against a topology that is still changing.

Today the only hand-written SIMD kernel is x86-only: the runtime dispatcher in engine/src/nnue/inference.rs selects the AVX2 kernel via is_x86_feature_detected, and on aarch64 the x86 block compiles out so Apple Silicon always falls back to the scalar loop. Add a NEON kernel mirroring the AVX2 semantics for the v2 bucketed multi-layer network, selected via is_aarch64_feature_detected runtime dispatch (the pattern from TASK-69.5), covering the SCReLU activation (TASK-86.2) and the bucketed output stack (TASK-86.4).

Strategic note: this is Apple-Silicon deployment/iteration speed, NOT a CCRL strength lever. TASK-82 established that raw speed is not the bottleneck and CCRL runs single-core x86/AVX2, so this is deliberately Low priority and must not gate critical-path eval work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A NEON aarch64 inference path evaluates the v2 bucketed multi-layer network, runtime-dispatched via is_aarch64_feature_detected and selected on Apple Silicon
- [ ] #2 The NEON path is bit-identical to the scalar reference, covered by extending the TASK-69.10 golden-vector / three-way differential equivalence test to aarch64
- [ ] #3 The kernel covers all v2 activations including SCReLU (TASK-86.2) and the bucketed output-stack tail (TASK-86.4)
- [ ] #4 Where the int8 dense tail allows, dotprod (sdot/udot) is used, with any i8mm opportunity noted
- [ ] #5 The NNUE design contract is updated to document an aarch64/NEON SIMD path, replacing its current x86-64/AVX2-only assumption
<!-- AC:END -->
