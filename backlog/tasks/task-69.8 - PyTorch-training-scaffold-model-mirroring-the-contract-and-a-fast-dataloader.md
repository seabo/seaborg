---
id: TASK-69.8
title: 'PyTorch training scaffold: model mirroring the contract and a fast dataloader'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 03:09'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Create Python/PyTorch training project under tools/trainer (sibling of tools/strength): requirements.txt (torch, numpy), README with throughput figures, .gitignore for venv/checkpoints, flat modules + stdlib unittest tests (matching the strength harness convention).
2. model.py: NnueModel mirroring the design contract — feature transformer 768->H per perspective as an nn.EmbeddingBag (weight [768,H] matches the on-disk feature-major W_ft layout), separate b_ft parameter, concat(acc[stm], acc[nstm]) side-to-move first, clipped-ReLU to [0,1] (float domain corresponding to [0,QA]), linear 2H->1 output. Config dataclass exposes the contract's parameterizable dims (hidden width H, activation id crelu/screlu, output SCALE, QA/QB) with H a positive multiple of 16.
3. data.py: fast dataloader over the SBRG packed format (8-byte header + 32-byte records). np.memmap the file, vectorized batch decode: unpack occupancy bits and piece nibbles, scatter nibbles to occupied squares via cumsum, compute per-perspective feature indices with the contract's index formula (oriented + 64*pt0 + 384*side), emit EmbeddingBag (indices, offsets) for stm and nstm perspectives sharing one offsets array. Decode targets: y = lambda*r + (1-lambda)*sigmoid(search_cp/SCALE).
4. train.py: training loop, MSE in win-probability space (sigmoid(fout) vs y), train/val split, reports train+val loss per epoch, saves an fp32 checkpoint. throughput.py (or a --benchmark flag): measure and record dataloader samples/sec.
5. Generate a small real self-play dataset with the engine datagen CLI; run a training run that shows loss converging on train and val; record the throughput figure and losses in the README.
6. Tests: decode correctness against hand-computed feature indices for known positions and against engine-generated records; model shape/parameterization; target formula. Run cargo fmt/clippy/test (no Rust source changed, guard against regressions) and the Python tests; write the review handoff.
<!-- SECTION:PLAN:END -->
