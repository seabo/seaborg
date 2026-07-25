---
id: TASK-86.3
title: NNUE architecture-sweep methodology (decision doc)
status: To Do
assignee: []
created_date: '2026-07-25 12:23'
labels:
  - design
dependencies:
  - TASK-86.1
parent_task_id: TASK-86
priority: high
ordinal: 145000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Before training many candidate architectures on the fixed corpus (TASK-81), pin the methodology as a decision record under docs/, so the sweep is fair and its results are trustworthy. Playing strength is a joint function of eval quality and search speed, so the objective is realized fixed-time-control Elo; the loss-vs-NPS frontier is a cheap screen that decides which few nets earn game matches. The doc must fix: the train/validation split protocol (held out by whole game/opening, not random position, to avoid leakage between correlated positions in one game); the quality axis (post-quantization-aware-training validation loss with lambda and loss fixed across runs); the cost axis (realized single-thread in-engine bench NPS using the incremental accumulator, not a standalone forward-pass microbenchmark); the screen -> Pareto-frontier -> fixed-TC SPRT funnel; and how to read the frontier's flattening as label-limited vs capacity-limited (a signal that the next lever is better labels, i.e. datagen node budget, rather than more parameters).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A decision document under docs/ specifies the by-game (not by-position) train/validation split protocol and the rationale (position correlation within a game)
- [ ] #2 It specifies the quality axis as post-QAT quantized validation loss with lambda, loss function, and training budget held fixed across candidates, and the cost axis as realized in-engine single-thread bench NPS via the incremental accumulator path
- [ ] #3 It specifies the screen (loss/NPS frontier) -> finalists -> fixed-time-control SPRT decision funnel, and states why static loss cannot be the final arbiter (eval quality changes the search tree)
- [ ] #4 It defines how to interpret frontier flattening as label-limited vs capacity-limited and what that implies for the next investment
<!-- AC:END -->
