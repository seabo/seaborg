---
id: TASK-69.8
title: 'PyTorch training scaffold: model mirroring the contract and a fast dataloader'
status: To Do
assignee: []
created_date: '2026-07-20 19:41'
labels:
  - nnue
  - training
  - python
dependencies:
  - TASK-69.1
  - TASK-69.7
parent_task_id: TASK-69
priority: high
ordinal: 110000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Stand up the Python/PyTorch training project (under tools/, alongside the existing strength harness) with a model whose architecture mirrors the design contract (TASK-69.1) and is parameterized over the dimensions the contract marks variable, and a dataloader for the packed sample format (TASK-69.7) that is fast enough not to starve the GPU. Because the network is tiny (order 10^5 parameters), training is dataloader-bound, so a naive per-sample Python loop is a real bottleneck; build sparse-feature batching over the packed format (memory-mapped, or a Rust/PyO3 reader) rather than a naive loader.

This task delivers a training run that consumes generated data and produces an fp32 checkpoint; the quantized export and the strength-preserving numeric guarantees are TASK-69.9 and TASK-69.10.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A PyTorch model matches the design-contract architecture and exposes the parameterizable dimensions as configuration
- [ ] #2 The dataloader reads the packed sample format and sustains a measured throughput high enough to keep the GPU utilized, with the figure recorded
- [ ] #3 A training run over a sample dataset converges to a checkpoint and reports training and validation loss
<!-- AC:END -->
