---
id: TASK-69.9
title: Blended WDL-and-score loss and quantization-aware network export
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 05:48'
labels:
  - nnue
  - training
  - python
dependencies:
  - TASK-69.8
  - TASK-69.2
parent_task_id: TASK-69
priority: high
ordinal: 111000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the training target and the export path. The loss blends the search score and the game WDL outcome with the lambda schedule from the design contract, so that the network is anchored to real game results and not only to its own predecessor. Implement quantization-aware handling so the values the exporter writes are values the training loop has already seen, then export a network file in the versioned format (TASK-69.2) that the engine loads directly.

The WDL term is the only signal in the whole loop that comes from the rules of chess rather than from the engine evaluation itself, so getting this blend right is what keeps the reinforcement loop anchored to reality.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The loss combines search-score and WDL targets with a configurable, schedulable lambda, covered by a test on a small fixture
- [ ] #2 Training accounts for quantization so exported integer weights reproduce the trained model behaviour within the contract tolerance
- [ ] #3 The exporter writes a versioned network file that the engine loader (TASK-69.2) accepts
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Lambda schedule (AC#1): add a LambdaSchedule to train.py supporting a constant lambda (default 0.3) and a linear ramp over generations (0.1->0.5), resolved per-generation; keep the float 'lam' path back-compatible. CLI gains --lambda-end/--lambda-generations/--generation. Test on a small fixture in test_train.py.
2. Quantization-aware training (AC#2): add straight-through fake quantization (round-half-to-even weights at QA/QB, activation at QA) to NnueModel behind a quantization_aware flag so the training forward already computes the quantized quantities; keep feature-transformer weight/bias magnitude clamping each step so the i16 accumulator cannot overflow for any <=32-piece position. train() enables QAT.
3. Exporter (AC#2/#3): export.py quantizes a trained model to integer Parameters (round-half-to-even; W_ft*QA i16, b_ft*QA i16, W_out*QB i16, b_out*QA*QB i32), verifies no integer-type overflow and the accumulator i16 bound, and serialises the SBNN file (64-byte header + blob + FNV-1a hash) byte-for-byte per engine/src/nnue/format.rs. A Python integer-inference reference mirrors engine::nnue::forward exactly; a fixture test asserts it reproduces the QAT float forward within a small documented centipawn tolerance.
4. Cross-language (AC#3): export.py --emit-fixture writes a deterministic patterned SBNN file; a Rust integration test (engine/tests) asserts engine::nnue::Network::read accepts it and decodes to the expected network. Python test independently re-reads the exported bytes and validates every header field.
5. Update README/model docstrings (export is now this task, not a later one). Run cargo fmt/clippy/test and the Python unittest suite; write the review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the blended-loss lambda schedule, quantization-aware training, and the SBNN exporter in tools/trainer; added a Rust integration test that loads an exported fixture. No engine source changed except the new integration test and its fixture.

AC#1 (schedulable lambda): train.py gains LambdaSchedule (constant, or a linear ramp resolved per reinforcement generation) and resolve_lambda; a run trains one generation and resolves to a single lambda. CLI flags --lambda-end/--lambda-generations/--generation build and resolve it. test_train.py pins the schedule arithmetic and its effect on the blended target on a small fixture (search vs outcome endpoints, linear interpolation, clamping, and that the schedule genuinely changes the target).

AC#2 (quantization-aware, reproduces within tolerance): model.py adds a straight-through fake-quantization forward (round-half-to-even weights at QA/QB, activations at QA) behind a quantization_aware flag, plus clamp_for_quantization() that bounds the feature-transformer weights so the i16 accumulator cannot overflow for any <=32-piece position. train() trains quantization-aware by default and clamps each step. export.py quantizes with the contract rounding and scales, refuses any weight overflowing its integer type or an accumulator that could exceed i16, and provides integer_eval_cp mirroring engine::nnue::forward exactly. Because training optimises the quantized behaviour, the exported integer network reproduces the model's own centipawn output to <=1 cp (measured max 0.49 cp over a trained fixture; the residual is only the final round-half-away divide). test_export.py asserts it.

AC#3 (engine loads the exported file): export.py serialises the 64-byte SBNN header + blob + FNV-1a hash byte-for-byte per engine/src/nnue/format.rs, with an independent Python reader (QuantizedNetwork.from_bytes) validating every field and rejection rule. export.py --emit-fixture writes a deterministic patterned network to engine/tests/fixtures/exported_v1.sbnn; engine/tests/loads_exported_network.rs loads it with Network::read and asserts it decodes to the network rebuilt from the same pattern.

Verified end to end: CLI train (lambda ramp at generation 2) -> checkpoint -> export.py -> reloadable SBNN file. README documents the schedule, QAT, and export.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 05:48
---
Implementation handoff
Branch: task-69.9-blended-loss-quantized-export
Worktree: /Users/seabo/seaborg-worktrees/task-69.9-blended-loss-quantized-export
Base: 027d20f3992a77e3d641c4c3acd3d553434e8d79
Implementation target: fb11aa41b13abec8539794bc09d43475a4129dba
Resolved findings: none
Verification:
- cargo fmt --check: pass (clean)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0, 0 warnings)
- cargo test --workspace: pass (49 chess + 379 engine incl. new loads_exported_network + 104 + 19 + 1s; 0 failed, 2 ignored)
- python -m unittest discover -p 'test_*.py' (tools/trainer, torch 2.13 / numpy 2.5 / Python 3.14 venv): pass (42 tests)
- End-to-end CLI: train.py (lambda ramp, generation 2) -> checkpoint -> export.py -> SBNN file reloadable; measured export reproduction max 0.49 cp over a trained fixture
Known failures: none
Notes for the reviewer: only tools/trainer changed plus one engine integration test (engine/tests/loads_exported_network.rs) and its committed fixture (engine/tests/fixtures/exported_v1.sbnn, 25 KB, regenerable via 'python export.py --emit-fixture ...'). No engine/src changed. The Python suite needs the venv deps (tools/trainer/requirements.txt); create tools/trainer/.venv and pip install -r requirements.txt. The .venv, checkpoints, and datasets are gitignored.
---
<!-- COMMENTS:END -->
