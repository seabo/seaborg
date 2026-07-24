---
id: TASK-69
title: NNUE evaluation trained end-to-end by self-play reinforcement
status: Done
assignee: []
created_date: '2026-07-20 19:39'
updated_date: '2026-07-24 10:41'
labels:
  - nnue
  - eval
  - architecture
  - strength
dependencies: []
priority: high
ordinal: 102000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella for building an NNUE (efficiently-updatable neural network) evaluation for Seaborg and the full pipeline that trains it, with a hard constraint that playing strength must be bootstrapped entirely from self-play: no external games, positions, or evaluations enter the system. The only priors permitted are internal — the current hand-crafted evaluation used to seed iteration 0, and the choice of feature set and architecture.

Goal. Replace (as a selectable, then default, evaluation) the tapered hand-crafted evaluation from TASK-64.14 with a quantized neural network evaluated incrementally in search, trained by distilling the engine's own search into successive network generations (an NNUE-style reinforcement loop, not AlphaZero MCTS).

Why now. The incremental-evaluation seam this depends on already exists: EvalState is threaded through Search via an eval_stack and driven by the PieceDeltaSink trait and Position::replay_last_move_deltas (TASK-64.15), with debug-time incremental-vs-from-scratch validation at every node. An NNUE accumulator is another consumer of that same per-move change set, so the hardest integration risk is already retired. This umbrella depends on the TASK-64 search-foundation programme: a network trained by distilling a weak search bakes that weakness into every label.

Structure. Work splits into four tracks that can proceed largely in parallel once the design contract (subtask .1) is fixed: Rust inference, Rust self-play data generation, Python training, and the reinforcement loop that ties them together. Each subtask is scoped to merge and be reviewed in isolation.

Scope. This task tracks the programme and carries no implementation of its own; it is complete when its children are Done or explicitly closed with a recorded decision.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every child task is Done or explicitly closed with a recorded decision not to pursue it
- [ ] #2 A closing summary records the measured strength delta of the first fully-trained network against the tapered hand-crafted evaluation baseline, using the repository strength-test harness
- [ ] #3 The self-play purity constraint (no external games, positions, or evaluations) is upheld across the whole pipeline and documented
- [ ] #4 The TASK-64 search-foundation programme is complete, or a recorded decision fixes the search baseline the training run distils from
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Bootstrap complete. Self-play reinforcement from the tapered hand-crafted eval produced gen-000 (+263 vs HCE), gen-001 (+157 vs gen-000), gen-002 (+22 vs gen-001); gen-003 regressed (-17) and the promote-on-improvement SPRT gate rejected it, so gen-002 is the final network and is baked as the default evaluator (TASK-69.13/.15). Direct gauntlet: gen-002 is ~337 Elo stronger than the hand-crafted evaluation (+334.1 and +339.6 in two independent runs at tc=10+0.1). Full per-generation curve and cost accounting in BENCHMARKS.md (TASK-69.12). The RL curve turned over at gen-3, so further NNUE gains require an architecture or training-data change rather than additional generations of the current loop.
<!-- SECTION:FINAL_SUMMARY:END -->
