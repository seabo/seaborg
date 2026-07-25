---
id: TASK-86.2
title: Add SCReLU activation support end-to-end for NNUE
status: To Do
assignee: []
created_date: '2026-07-25 12:23'
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
