---
id: TASK-69.12
title: Run the bootstrap programme and measure per-iteration strength
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:42'
updated_date: '2026-07-22 03:12'
labels:
  - nnue
  - rl
  - strength
dependencies:
  - TASK-69.11
  - TASK-69.5
  - TASK-64.22
parent_task_id: TASK-69
priority: medium
ordinal: 114000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Execute the reinforcement loop (TASK-69.11) for the initial programme of iterations using the SIMD inference path (TASK-69.5) for throughput, and record the outcome. Measure strength after each iteration against both the previous best and, where feasible, an external fixed reference via the existing gauntlet harness, so the curve is anchored to an absolute scale and not only to self-play deltas. Capture the realised datagen throughput and training cost against the earlier estimates, and record where the strength curve begins to flatten.

The deliverable is evidence: the trained network that becomes the new default evaluation, plus a recorded strength curve and cost accounting for the programme.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The loop runs for the planned iterations and produces a network that passes its strength gate and becomes the default evaluation
- [ ] #2 Per-iteration strength is recorded against the previous best, and against an external reference where feasible, with results archived per the strength-testing docs
- [ ] #3 Realised datagen throughput and training cost are recorded and compared against the pre-run estimates
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Provision the compute host (rig 'seaborig', Ryzen 9 3900XT 12c/24t, 32GB, RTX 2070 SUPER): clone master d53e33e, rustup stable, prebuilt FastChess v1.7.0-alpha, uv venv with CUDA torch. Nothing on the host is a training input; it only runs the repo's own tools.
2. Calibrate before committing compute: measure datagen positions/s against worker count and node budget, training epoch cost on the GPU, and gate games/hour at the chosen time control. Fix the run parameters from measurement, not from guesses.
3. Smoke the whole loop end to end (--mode smoke) to prove datagen -> train -> export -> gate -> promote works on the host before any authoritative iteration.
4. Run the authoritative programme one generation at a time, with an SPRT gate that demands improvement (elo0/elo1 set for a gain, not merely non-regression), archiving each generation's gate report and ledger line.
5. Anchor the final promoted network against the hand-crafted evaluation with a separate equal-time gauntlet, so the curve has an absolute reference and not only self-play deltas.
6. Record the realised throughput and cost against the pre-run estimates, land the strength curve in BENCHMARKS.md and the run in docs, and make the promoted network the default evaluation.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Compute host provisioned and calibrated (rig 'seaborig': Ryzen 9 3900XT 12c/24t, 32GB, RTX 2070 SUPER, Manjaro). Toolchain: rustup stable 1.97.1 (the distro rust was 1.71.1, below the 1.93 workspace floor), FastChess v1.7.0-alpha prebuilt linux-x86-64 release binary, uv venv on Python 3.12 with torch 2.13.0+cu130 (CUDA available on the 2070 SUPER). Repo transferred as a git bundle of master d53e33e, so nothing was pushed. A smoke iteration (--mode smoke --limit depth=4 --max-games 4) completed the whole datagen -> train -> export -> gate -> promote path in 8.4s and wrote a well-formed ledger line.

Measured cost, replacing the pre-run estimates:

Datagen (hand-crafted evaluator, --nodes 5000 --filter-opening-plies 8 --opening-plies 6):
- 3855 raw positions/s, 2552 kept samples/s = 9.19M kept samples/hour, 77.8 kept samples/game, 32 bytes/sample.
- Worker count matters far more than expected. Throughput peaks at the physical core count and then collapses: 6w 3627 pos/s, 8w 4736, 10w 4823, 12w 4790, 14w 3774, 16w 3263, 20w 2461, 24w 2116. Running the 24 available threads is 2.25x slower than running 12. The run uses --workers 12.
- At 24 workers throughput was also flat across node budgets (2057 pos/s at 500 nodes vs 2173 at 10000), i.e. the machine was saturated on something other than search. At 12 workers it scales with the budget as expected (3000n 4768, 6000n 3357, 10000n 2155, 20000n 1091), which is what fixed the diagnosis. TT size is not involved: 1/4/16/64 MB per worker are within noise of each other.

Training: not a bottleneck. Dataloader 502,817 samples/s, which bounds the epoch; 25 epochs over a 30M-sample generation is ~25 min. The GPU is idle most of that time.

Gate: 44 games at tc=10+0.1 with --concurrency 11 took 160s = 990 games/hour. A gate resolving in ~2000 games is ~2h; the 10000-game cap is ~10h.

Generation 0 launched on the host: 386,000 games at 5000 nodes (~30M samples, ~3.3h), H=256, 25 epochs, lambda ramp 0.1 -> 0.5 over 8 generations, gate tc=10+0.1 concurrency 11 against the hand-crafted evaluation.

Two observations on the loop worth recording. (1) loop.py passes no per-generation opening seed, so a multi-iteration run replays the same diversification openings every generation; this run drives one iteration per invocation with an explicit --opening-seed to avoid that. (2) The gate inherits strength_test.py's elo0=-5/elo1=0 default, which is a non-regression test rather than a demand for improvement; that is the right bound for the generation-0 bootstrap but should be tightened for later generations.
<!-- SECTION:NOTES:END -->
