---
id: TASK-86.8
title: Parallelize the NNUE packed dataloader across CPU workers
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-27 22:47'
updated_date: '2026-07-27 22:48'
labels:
  - nnue
  - tooling
dependencies: []
parent_task_id: TASK-86
priority: high
type: task
ordinal: 158000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The packed self-play dataloader (tools/trainer/data.py, iter_batches) decodes each batch on a single thread, and data.py itself notes that "the network is tiny ... so training is dataloader-bound". On the training host this leaves the GPU at roughly 19% utilization while one epoch over the ~93M-record gen-002 corpus takes about 3 minutes, so any architecture sweep or RL retraining is bottlenecked on a single core: a 14-candidate, multi-epoch sweep becomes an overnight job purely because batch decoding does not use the other cores.

Parallelize batch decoding across multiple CPU workers so training throughput scales with core count. The trained result must not change: each batch is already a pure function of its slice of shuffled record indices, so the parallel loader must yield exactly the same batches in exactly the same order as the single-thread loader for a given seed, split, and batch size. This determinism is a hard requirement, not a nicety, because the architecture sweep trains many candidates under a fixed-everything-but-architecture protocol and compares their validation losses; a loader that reordered or altered batches between runs would silently confound the comparison. Preserve the by-shard split, the sparse EmbeddingBag encoding, and the exact numeric batch contents; the only change is that the decode work is spread across workers with results delivered in order.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The dataloader decodes batches using a configurable number of CPU workers, and measured --benchmark throughput scales substantially with worker count on the multi-core training host (record the before/after samples-per-second and the speedup)
- [ ] #2 For a fixed seed, split, and batch size, the parallel loader yields the same batches in the same order as the single-thread loader (identical decoded tensors); a test asserts this equivalence, and a single worker reproduces the current behavior exactly
- [ ] #3 End-to-end training (train.py) on a small fixture corpus produces the same per-epoch val_loss trajectory (within floating-point tolerance) with the parallel loader as with the single-thread loader
- [ ] #4 Worker count is a CLI flag on train.py threaded through the sweep training config, with a sensible default; tools/trainer/README.md documents it and the measured speedup
- [ ] #5 cargo/repo-required checks and the trainer test suite pass; new tests cover the worker path, in-order determinism, and the single-worker fallback
<!-- AC:END -->
