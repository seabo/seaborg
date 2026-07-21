---
id: TASK-69.8
title: 'PyTorch training scaffold: model mirroring the contract and a fast dataloader'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 03:21'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented tools/trainer, the Python/PyTorch training project (sibling of tools/strength), covering the float side of the NNUE design contract. No Rust source was changed.

model.py — NnueModel mirrors the contract topology: a shared feature transformer 768->H per perspective as an nn.EmbeddingBag whose weight is [input_dim, H], exactly the feature-major on-disk W_ft order (so quantized export serialises it untransposed); a separate ft_bias; concat(acc[stm], acc[nstm]) side-to-move first; clipped ReLU; a 2H->1 linear output emitting fout in SCALE-normalised units (fout == eval_cp/SCALE). NnueConfig carries the contract's parameterizable dimensions (hidden H, activation crelu/screlu, scale, qa, qb) and applies the loader's validation rules (H a positive multiple of 16, output_dim 1, feature_set_id 0 => input_dim 768, positive scales). FT weights init small so the accumulator starts inside the CReLU active band.

data.py — the fast dataloader over the SBRG packed format (8-byte header + 32-byte records). It memory-maps the file and decodes a whole batch at once with vectorised NumPy: unpack occupancy bits and piece nibbles, scatter nibbles to occupied squares via a cumsum of occupancy, compute per-perspective feature indices with the contract formula (oriented + 64*pt0 + 384*side), then flatten under the occupancy mask into the (indices, offsets) form EmbeddingBag consumes. stm/nstm share one offsets array because they cover the same active squares. No per-sample Python loop. Score (raw i16, mate band preserved) and wdl are passed through untouched; the trainer owns the target formulation.

train.py — the blended win-probability target y = lambda*r + (1-lambda)*sigmoid(search_cp/SCALE) with p = sigmoid(fout), MSE loss; a train/val split reporting both losses per epoch; fp32 checkpoint writing (config + float state_dict + loss history); and a --benchmark mode measuring decode throughput.

testsupport.py + test_data.py + test_model.py — stdlib unittest (no pytest dep, matching the strength harness). An independent reference encoder builds records so tests are hermetic and do not require the engine to be built. Feature indices are checked against hand-computed values from the contract formula; perspective selection, target blend, stream-header rejection, and the architectural mirror invariance (a position and its colour/board mirror evaluate identically from the side to move) are all covered; a short run is asserted to converge.

Verified end to end against real engine data: seaborg datagen produced 216,233 filtered self-play samples; the loader benchmarked at ~561,000 samples/sec (decode only, batch 16,384) versus ~197,000 samples/sec for the full CPU training step, so the loader has ~2.8x headroom over a CPU trainer and more over a GPU. A 25-epoch run converged monotonically (train 0.0462 -> 0.0016, val 0.0309 -> 0.0040) to a 197,377-parameter fp32 checkpoint whose weight shapes match the contract (feature_transformer.weight [768,256], output.weight [1,512]). Figures recorded in tools/trainer/README.md. The .venv and generated *.bin/*.pt artifacts are gitignored.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 03:21
---
Implementation handoff
Branch: task-69.8-pytorch-training-scaffold
Worktree: /Users/seabo/seaborg-worktrees/task-69.8-pytorch-training-scaffold
Base: 05880a59a02a47f388fafad164e482fb764c7ccc
Implementation target: 5bcc8889e7f2dd1a2ca1b31601a0bff317167b71
Resolved findings: none
Verification:
- cargo fmt --check: pass (clean)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (540 tests: 49+373+98+19+1; 2 ignored; 0 failed)
- python -m unittest discover (tools/trainer, torch 2.13/numpy 2.5, Python 3.14 venv): pass (22 tests)
- Real-data run: seaborg datagen -> 216,233 samples; loader ~561k samples/sec decode vs ~197k/sec full CPU step; 25-epoch training converged train 0.0462->0.0016, val 0.0309->0.0040; fp32 checkpoint (197,377 params) written
Known failures: none
Notes for the reviewer: no Rust source changed (only tools/trainer added), so the Rust checks guard against regressions rather than exercise new code. The Python suite requires the venv deps (torch, numpy) per tools/trainer/requirements.txt; create tools/trainer/.venv and pip install -r requirements.txt to run them. The generated dataset, checkpoint, and venv are gitignored and not committed.
---
<!-- COMMENTS:END -->
