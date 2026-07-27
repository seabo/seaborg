---
id: TASK-86.8
title: Parallelize the NNUE packed dataloader across CPU workers
status: In Review
assignee:
  - '@george'
created_date: '2026-07-27 22:47'
updated_date: '2026-07-27 23:06'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Parallel-decode the packed dataloader via a thread pool, threaded through the trainer and the sweep.

What changed:
- data.py: new BatchLoader decodes batch slices across worker threads and yields them in submission order. Each batch is a pure function of its slice of the (already shuffled) index array, so the batch sequence — and thus the training trajectory — is identical to the serial loader for any worker count; only wall time changes. In-flight tasks are capped at 3x the worker count so a fast pool cannot buffer a whole epoch of multi-MB batches. num_workers<=1 delegates to the unchanged iter_batches.
- train.py: train(), _evaluate(), and benchmark_dataloader() run through one reused BatchLoader (pool created once per run); new --num-workers flag (default min(8, cpu_count)).
- sweep.py: TrainingConfig.num_workers held fixed across candidates, exposed as --num-workers, passed to train.py.

Threads, not processes: both plateau at the same rate here because decode is memory-bandwidth bound; threads avoid per-batch pickling and the fork-after-CUDA-init hazard, and share the memmap. NumPy releases the GIL for the vectorised decode, so threads run it concurrently.

Measured (rig, AMD Ryzen 9 3900XT 12c, CPU decode, one ~5.1M-sample shard, batch 8192): 1 worker 574k samples/s -> 8 workers 1.445M/s (~2.5x); 12 workers 1.285M/s (declining). The realized speedup is ~2.5x, not larger: decode is memory-bandwidth bound and a few threads saturate it. This turns the full-corpus epoch from ~3 min toward ~1.2 min.

Tests (all in the trainer venv, python -m unittest): test_data ParallelLoaderTest (parallel==serial over a ragged split, single-worker fallback, in-order under a prefetch cap that drains mid-stream); test_train ParallelTrainingEquivalenceTest (num_workers 1 vs 3 give an identical val_loss trajectory). 116 trainer tests pass.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-27 23:05
---
Implementation handoff
Branch: task-86.8-parallel-dataloader
Worktree: /Users/seabo/seaborg-worktrees/task-86.8-parallel-dataloader
Base: 644153ae4d9413cc0243e1211f0ce7641719979a
Implementation target: dc6b34628e9ac910bf5908e274705f73cf2acf19
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings)
- cargo test --workspace: pass (10 suites ok, 0 failed)
- trainer suite (python -m unittest test_data test_train test_sweep test_model test_export test_topology_v2 test_split): 116 passed
- train.py --benchmark on one ~5.1M-sample shard, batch 8192: 574k (1w) -> 1.445M (8w) samples/s, ~2.5x
Known failures: none
Note: Rust is untouched (Python-only change under tools/trainer); the required cargo checks were run and pass regardless.
---
<!-- COMMENTS:END -->
