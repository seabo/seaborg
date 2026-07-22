---
id: TASK-69.12
title: Run the bootstrap programme and measure per-iteration strength
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:42'
updated_date: '2026-07-22 21:47'
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

Generation 0 result: PASS, promoted, +263.2 +/- 32.7 Elo over the hand-crafted evaluation.

Attribution: 386,000 self-play games at 5000 nodes/move produced 31,409,859 filtered samples; H=256, 25 epochs, lambda 0.1; candidate nnue:gen-000:sha256=5a532e9d7d89af1c. Gate was authoritative at tc=10+0.1, concurrency 11, openings seaborg-openings-v1, SPRT elo0=-5 elo1=0 alpha=beta=0.05, crossing the upper bound (LLR 2.95) after 358 games: 247 wins, 93 draws, 18 losses, pentanomial [0, 2, 27, 69, 81].

The result was checked for the ways a large Elo delta is usually an artifact rather than a gain. Both sides are the same binary (sha256 7ddae8ac...), differing only in the candidate's EvalFile option, so the comparison isolates the evaluation. All 358 games terminated normally: zero crashes, disconnections, forfeits, illegal moves, or losses on time, so the margin is not a baseline failing to play. Preflight had both sides answering g1f3. The limit is a time control, not a node budget, so this is not the free-depth artifact that inflates search-change measurements.

Realised cost against the pre-run calibration: datagen ran as predicted at ~9.2M kept samples/hour and took the bulk of the wall clock; the gate resolved in 358 games (~22 min) rather than the ~2h a marginal change would need, because the effect is large. Total generation-0 wall clock was about 3.5h.

Generation 1 launched: 360,000 games seeded differently (--opening-seed 2000), self-play now evaluated by best.sbnn. Datagen with NNUE inference measured at 2965 raw positions/s against 3855 for the hand-crafted evaluation, 23% slower, giving 7.12M kept samples/hour and a ~4.2h datagen step. Its gate is tightened to elo0=0 elo1=5, so a candidate must show improvement rather than merely avoid regressing.

Generation 1 result: PASS, promoted, +156.5 +/- 26.1 Elo over generation 0.

Attribution: 360,000 self-play games evaluated by nnue:gen-000, 5000 nodes/move, 44,133,929 raw positions filtered to 29,532,992 samples in 15,134s (2916 positions/s, against 2965 measured on the pre-run probe, so the cost model held at full scale). Training converged by roughly epoch 19 of 25 (final val_loss 0.004424). Candidate nnue:gen-001:sha256=8ebc1381ca166774. Gate at tc=10+0.1 concurrency 11 under the tightened SPRT (elo0=0, elo1=5) crossed the upper bound after 476 games: 259 wins, 159 draws, 58 losses, pentanomial [5, 15, 58, 94, 66], 0 crashes, 0 forfeits, all terminations normal.

The tightened bound did what it was meant to: unlike generation 0 it demanded evidence of improvement rather than mere non-regression, and the candidate supplied it.

Note that generation 1's val_loss (0.004424) is higher than generation 0's (0.002736) and the two are not comparable. The models fit different targets: generation 1's labels come from an evaluator 263 Elo stronger, and the schedule moved lambda from 0.10 to 0.15. Only the gate compares generations meaningfully.

Curve so far, each measured against the immediately preceding best at the same time control: gen 0 +263.2 over the hand-crafted evaluation, gen 1 +156.5 over gen 0. Still climbing steeply; no sign of flattening at two generations.

Operational correction: the host idled 5h16m after generation 1 finished (17:30 BST) because each generation was being launched by hand and the watching process died. Generations 2 through 7 now run from ~/rl/chain.sh, which invokes loop.py once per generation with a distinct opening seed and stops the chain only on an infrastructure failure, treating a failed or inconclusive gate as a normal outcome that the next generation still follows.
<!-- SECTION:NOTES:END -->
