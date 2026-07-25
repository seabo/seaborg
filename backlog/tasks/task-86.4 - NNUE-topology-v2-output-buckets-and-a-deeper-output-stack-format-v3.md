---
id: TASK-86.4
title: 'NNUE topology v2: output buckets and a deeper output stack (format v3)'
status: To Do
assignee: []
created_date: '2026-07-25 12:23'
labels:
  - nnue
dependencies:
  - TASK-86.1
parent_task_id: TASK-86
priority: high
ordinal: 146000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the network beyond a single hidden layer to a modern topology while keeping the existing 768 feature set (no feature-set change): a wider feature transformer feeding a deeper output stack (e.g. 2H -> 16 -> 32 -> 1) with piece-count-selected output buckets (e.g. 8 buckets), where only the selected bucket's tail executes per evaluation. The feature transformer dominates per-node cost and is maintained incrementally, so the small dense tail and extra buckets add eval capacity at near-zero runtime cost. Requires a versioned file-format bump for the extra layers and buckets, int8 quantization for the dense tail, matching PyTorch training/export, and Rust scalar+SIMD inference with bucket selection. This is a topology-only change that de-risks the training/quant/inference path for a bigger net before the king-bucketed feature set is attempted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The file format is versioned to carry a multi-layer output stack and an output-bucket count, with deterministic rejection of files whose architecture fields are unknown or mismatched
- [ ] #2 Inference selects exactly one output bucket by a documented static rule (e.g. piece count) and evaluates only that bucket's tail; scalar and AVX2 paths are bit-identical
- [ ] #3 The PyTorch model, quantization-aware training, and export produce a valid bucketed multi-layer .sbnn (int8 dense tail) that the engine loads and evaluates
- [ ] #4 A golden-vector / three-way differential equivalence test covers a bucketed multi-layer network across positions spanning multiple buckets
<!-- AC:END -->
