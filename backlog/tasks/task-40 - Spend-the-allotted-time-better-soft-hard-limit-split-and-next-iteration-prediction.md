---
id: TASK-40
title: >-
  Spend the allotted time better: soft/hard limit split and next-iteration
  prediction
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-18 12:17'
updated_date: '2026-07-22 13:10'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation complete; strength measurement in progress.

Allocation. `TimeControl::to_move_budget` returns `MoveBudget { optimum, maximum }`. The optimum is byte-for-byte the figure `to_move_time` always produced — `to_move_time` now delegates to it, and a test asserts the two agree across the full clock/increment/movestogo grid, so the TASK-7 overflow tests and TASK-38 proportional-allocation tests keep pinning exactly what they pinned before. The maximum multiplies the *untrimmed* allocation by 3 and then applies the same `MAX_CLOCK_SHARE_DIVISOR` cap, which means a move the cap already trimmed gets no extension at all. `go movetime` returns optimum == maximum.

Plumbing. `SearchLimit::Time` carries a `TimeBudget { soft, hard }` (`TimeBudget::fixed` for the strict case; `new` raises a hard below soft so `soft <= hard` holds by construction). At thread spawn both resolve against one clock read into `Deadlines`. `Search::stopping()` is unchanged and still tests only the hard deadline, so TASK-32's guaranteed-first-ply suppression and TASK-39's cancellation responsiveness are untouched by construction, not by re-verification.

Prediction. `IterationCost` keeps the last two iteration durations; `predict_next` extrapolates the observed ratio, clamped to [1.5, 8.0], and withholds an estimate entirely when the earlier of the two ran under 500us (where clock resolution dominates) or when fewer than two have completed. A withheld estimate leaves the loop ungated, which is the pre-change behaviour, so the first ply can never be declined.

Instability. After each iteration, `instability_scale` combines a changed root best move (+0.6) with the root score drop (drop/150cp, capped at +1.0) into a multiplier on the soft limit, clamped by the hard deadline in `next_iteration_fits`.

Observed effect (startpos, `go wtime 60000 winc 500`, optimum 1997ms): baseline completes depth 16 at 1623ms then burns to ~2000ms on a depth-17 iteration it discards. The candidate declines depth 16, returns the same move e2e4 at depth 15 in ~1150ms, and hands ~850ms back to later moves.

Checks: `cargo fmt --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace` 622 passed / 0 failed.

Strength, 2+0.05 (AC#5, first of two controls). Round-robin SPRT against the merge-base build, recorded in BENCHMARKS.md.

Baseline git:108c2bd (sha256 3e8b798c...), candidate git:7b474d2 (sha256 5bc910a7...), fastchess alpha 1.5.0, openings-v1.epd, tc=2+0.05, 64MB hash, one worker per engine, concurrency 4, Apple M3 Pro.

PASS: LLR 2.96 crossed the +2.94 boundary at 614 games. Elo +92.1 +/- 19.7 (pentanomial). W-D-L 255-263-96, pentanomial 6-34-108-113-46. Zero crashes, zero forfeits; all 614 games carry Termination "normal" and the runner log contains no illegal-move, forfeit, disconnect or timeout line — the harness fails closed on any of these before recording a result.

Artifacts: /tmp/task40-tc2/{report.json,runner.log,games.pgn}.

The 10+0.1 control is running now, sequentially rather than concurrently so the two matches do not contend for cores.

Strength, 10+0.1 (AC#5, second of two controls). Same two binaries, run sequentially after the 2+0.05 match so the two could not contend for cores.

PASS: LLR 2.95 crossed the +2.94 boundary at 722 games. Elo +76.8 +/- 18.7 (pentanomial). W-D-L 279-321-122, pentanomial 11-52-116-133-49. Zero crashes, zero forfeits; every game carries Termination "normal" and the runner log contains no illegal-move, forfeit, disconnect or timeout line.

Artifacts: /tmp/task40-tc10/{report.json,runner.log,games.pgn}.

Both controls are non-negative and both cross the no-regression boundary, so AC#5 is met at 2+0.05 (+92.1) and 10+0.1 (+76.8). The intervals overlap, so the smaller figure at the slower control is the expected shape — the discarded iteration is a smaller fraction of a longer move — rather than a measured difference between the controls. Both are recorded in BENCHMARKS.md.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-22 13:10
---
Implementation handoff
Branch: task-40-soft-hard-time-limits
Worktree: /Users/seabo/seaborg-worktrees/task-40-soft-hard-time-limits
Base: 108c2bd1ee99692d9c7bba9149cbbc4d34f772f2
Implementation target: f727536546e5221c35a11eb6dd9a1a84f3bdb86a
Resolved findings: none (first implementation attempt)
Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings suppressed and no new #[allow]
- cargo test --workspace: 622 passed, 0 failed, 2 ignored (pre-existing ignores)
- self-play SPRT tc=2+0.05 vs the base build: PASS, LLR 2.96, Elo +92.1 +/- 18.7 over 614 games, 0 forfeits, 0 crashes, 0 illegal moves
- self-play SPRT tc=10+0.1 vs the base build: PASS, LLR 2.95, Elo +76.8 +/- 18.7 over 722 games, 0 forfeits, 0 crashes, 0 illegal moves
Known failures: none

Notes for the reviewer. The optimum half of the new budget is asserted equal to the previous to_move_time across the whole clock/increment/movestogo grid, so the TASK-7 and TASK-38 allocation tests pin exactly what they pinned before. Search::stopping() is unchanged and still tests only the hard deadline, so TASK-32's guaranteed-first-ply suppression and TASK-39's cancellation responsiveness hold by construction rather than by re-verification. The self-play binaries are at /tmp/task40-builds (sha256 3e8b798c... baseline, 5bc910a7... candidate) with full reports under /tmp/task40-tc2 and /tmp/task40-tc10; these are outside the repository and will not survive a reboot, so the durable record is the BENCHMARKS.md section committed on this branch.
---
<!-- COMMENTS:END -->
