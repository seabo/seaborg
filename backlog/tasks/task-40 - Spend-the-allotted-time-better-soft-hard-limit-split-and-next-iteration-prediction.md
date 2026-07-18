---
id: TASK-40
title: >-
  Spend the allotted time better: soft/hard limit split and next-iteration
  prediction
status: To Do
assignee: []
created_date: '2026-07-18 12:17'
labels:
  - engine
  - time
  - search
dependencies:
  - TASK-38
priority: medium
type: feature
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-38 fixed how much time the engine is ALLOTTED per move. This ticket is about how well it SPENDS that allotment. The two are independent: a correct allocation is still wasted if the search throws the work away.

The search converts the allotted time into a single absolute deadline once, at thread spawn (engine/src/search.rs:152-158), stores it in Search::stop_time, and checks it only through Search::stopping() (engine/src/search.rs:767). There is exactly one limit, and the iterative-deepening loop (engine/src/search.rs:441-462) is a bare 'for d in 1..=depth { if self.stopping() { break; } ... }'.

Two consequences:

1. No soft/hard limit distinction. Established engines carry an 'optimum' time and a larger 'maximum' time, spending past the optimum only when the position warrants it: the root score is falling, the best move just changed, or the current iteration is unstable. seaborg has no way to express 'this position deserves more than its slice', nor to cut a search short when the best move is obvious.

2. No prediction of whether the next iteration fits. The loop starts iteration d+1 whenever the deadline has not yet passed, even when the budget clearly cannot accommodate it. Because an aborted iteration returns Score::zero() and is discarded without committing (engine/src/search.rs:454-462, 492, 716), that work is thrown away entirely. Iteration cost grows roughly geometrically, so the common case is starting an iteration with a small fraction of its cost remaining. The usual remedy is to skip iteration d+1 unless the elapsed time is below some fraction of the optimum, using the observed branching factor from previous iterations.

Both were identified during the TASK-38 investigation and deliberately left out of that ticket's scope, which was confined to the allocation policy in engine/src/time.rs. The design question here is genuinely open and should be settled before implementing: in particular whether SearchLimit::Time should carry a pair of durations, and how a soft-limit extension interacts with the TASK-32 min_search_complete guarantee and with the TASK-39 stop-responsiveness question.

Strength impact should be measured, not assumed. TASK-27 tooling and the TASK-38 self-play evidence give a usable baseline at 2+0.05, 10+0.1 and 1+0.01.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A soft/hard (optimum/maximum) time limit distinction exists and is plumbed from allocation through to the search
- [ ] #2 The iterative-deepening loop declines to start an iteration it predicts cannot complete within the budget, using a measured branching-factor estimate rather than a fixed constant
- [ ] #3 The search may exceed the soft limit under defined instability conditions (at minimum: best move changed at the root, or root score dropping) and never exceeds the hard limit
- [ ] #4 TASK-7 overflow safety, TASK-32 guaranteed-legal-move behavior, and TASK-38 proportional allocation all still hold, evidenced by their existing regression tests passing
- [ ] #5 A self-play match against the pre-change build at 2+0.05 and 10+0.1 shows a non-negative Elo delta with zero time forfeits and zero illegal moves
<!-- AC:END -->
