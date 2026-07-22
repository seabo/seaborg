---
id: TASK-40
title: >-
  Spend the allotted time better: soft/hard limit split and next-iteration
  prediction
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-18 12:17'
updated_date: '2026-07-22 02:57'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Allocation (engine/src/time.rs): add `MoveBudget { optimum, maximum }` and `TimeControl::to_move_budget`. The optimum is exactly today's `to_move_time` (which stays, delegating to the budget, so the TASK-7/TASK-38 allocation tests keep pinning it byte-for-byte). The maximum is a multiple of the optimum, clamped by the same max-clock-share cap the optimum already obeys, so the hard limit can never ask for more of the clock than the existing overflow-safe cap allows. `go movetime` stays strict: optimum == maximum.
2. Plumbing (engine/src/search.rs): replace `SearchLimit::Time(Duration)` with `SearchLimit::Time(TimeBudget)` carrying soft and hard durations (`TimeBudget::fixed` for the strict case). Convert to a `Deadlines { soft, hard }` pair of `Instant`s at thread spawn. `Search::stopping()` keeps using the hard deadline only, so the abort path — and therefore TASK-32's guaranteed-first-ply and TASK-39's stop responsiveness — is unchanged.
3. Iteration prediction (`Search::iterative_deepening`): time each completed iteration, derive a branching factor from the ratio of the last two iteration costs (clamped, and only once two real samples exist), and decline to start iteration d+1 when `elapsed + cost_d * ebf` exceeds the effective soft deadline. With fewer than two samples the loop is ungated, as today.
4. Instability extension: after each iteration compute whether the root best move changed and how far the root score dropped, and scale the soft deadline up by a bounded factor, clamped to the hard deadline. This is what lets the prediction gate spend past the optimum only where the position warrants it.
5. Update callers: engine/src/engine.rs, engine/src/game.rs, lichess/src/game.rs, benches/search.rs, engine/tests/timed_selfplay.rs and the search/uci unit tests.
6. Tests: unit tests for the budget arithmetic (optimum unchanged, maximum bounded by the clock-share cap, movetime strict), for the branching-factor gate (deterministic, injected iteration costs), and for the instability scale. Assert the hard deadline is never exceeded and the existing TASK-7/32/38 regression tests still pass.
7. Required checks (fmt, strict clippy, workspace tests), then a self-play round robin against the merge-base build at 2+0.05 and 10+0.1 recorded in BENCHMARKS.md.
<!-- SECTION:PLAN:END -->
