---
id: TASK-42
title: >-
  Hold a meaningful time reserve in increment games instead of decaying to the
  increment
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 13:18'
updated_date: '2026-07-19 12:51'
labels:
  - engine
  - time
  - search
dependencies:
  - TASK-38
priority: low
type: bug
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In an increment game the engine's clock decays geometrically over the course of the game until the per-move allocation collapses onto the increment itself, leaving a reserve of only tens of milliseconds. From roughly move 60 onward at fast controls the engine is effectively playing increment-only, which costs real strength in the late middlegame and endgame.

TASK-38 fixed the opening (the flat buffer no longer starves the first ~13 moves). This ticket is about the other end of the game and is a separate defect in the same allocation policy, engine/src/time.rs.

## Mechanism

to_move_time computes (clock - MOVE_OVERHEAD) / est_remaining_moves + inc, where est_remaining_moves is floored at MINIMUM_REMAINING_MOVES = 20. Past move 20 the estimate never rises above that floor, so the engine perpetually plans to divide its remaining base over 20 more moves and spends roughly 1/20 of the base every move, forever. The base therefore drains geometrically rather than being spent down deliberately over an expected game length.

The system does converge rather than flag: it has a fixed point where the allocation equals the increment and the clock stops falling. But it converges to a very thin reserve.

## Evidence

Simulating the merged formula (self-play, no time lost to anything but allocation):

    1+0.01   move  20: alloc  35ms, clock  505ms
             move  60: alloc  13ms, clock  100ms
             move 100: alloc  10ms, clock   49ms   <- equilibrium, 19ms above MOVE_OVERHEAD
    2+0.05   move 100: alloc  50ms, clock   96ms
    10+0.1   move 100: alloc 100ms, clock  163ms

Corroborated by the TASK-38 review's own FastChess run: at 1+0.01, 5 of 5569 moves were played at depth 1, all at moves 77-120 of long games, with the allocation floored at 10-11ms.

## Why this is worth fixing

At equilibrium the engine holds ~19ms of slack above the fixed overhead at 1+0.01. That is enough to avoid flagging in like-for-like self-play over local pipes, but it is thin against a real GUI, a loaded machine, or any search overshoot, and it means the engine has no capacity to think longer about a critical late position. A human would bank time in an increment game; the engine spends it down to nothing and then plays hand-to-mouth.

The conventional remedy is to treat the increment as funding the steady state and the base as a separate pool spent over an expected number of remaining moves, holding an explicit reserve floor rather than letting the base asymptote to zero. Options worth evaluating: raise or make dynamic MINIMUM_REMAINING_MOVES; allocate as inc + (clock - reserve) / est_remaining_moves for an explicit reserve; or adopt a standard base/increment split.

## Scope notes

Distinct from TASK-40, which concerns how well a single move SPENDS its allotment (soft/hard limits, next-iteration prediction). This ticket concerns how much is allotted across the whole game. The two interact and TASK-40 may land first; whichever is second should re-measure.

Do not regress TASK-7 (overflow safety), TASK-32 (guaranteed legal move under a zero budget) or TASK-38 (proportional allocation, no depth-1 openings at fast controls).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The per-move allocation in an increment game converges to a state that retains a defined, non-trivial time reserve rather than decaying until the reserve is a small multiple of MOVE_OVERHEAD, with the target reserve expressed as an explicit policy rather than emerging by accident
- [ ] #2 Unit tests simulate a full game at 1+0.01, 2+0.05 and 10+0.1 and assert the clock at moves 60, 100 and 140 stays above a defined reserve floor
- [ ] #3 The engine can still allot materially more than the increment to a late-game move when the clock allows, demonstrated by a test over a representative late-game clock state
- [ ] #4 TASK-7 overflow safety, TASK-32 guaranteed-legal-move behavior and TASK-38 proportional opening allocation all still hold, evidenced by their existing regression tests passing
- [ ] #5 A FastChess self-play match against the pre-change build at 1+0.01 and 2+0.05 shows a non-negative Elo delta, zero time forfeits, zero illegal moves, and a reduction in depth-1 moves played after move 60
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make the reserve an explicit policy in engine/src/time.rs: hold back RESERVE_INCREMENT_MOVES (10) moves' worth of increment from the clock, and spend only the surplus above that reserve over est_remaining_moves. Reserve = inc * RESERVE_INCREMENT_MOVES, so it is zero in sudden death (no flat buffer, TASK-38's proportionality property is untouched) and scales with the increment that funds the steady state.
2. Give the reserve a restoring force: below the reserve, allot usable/RESERVE_INCREMENT_MOVES, which is provably less than the increment there, so the clock climbs back toward the reserve instead of creeping past it under search overshoot.
3. Keep MOVE_OVERHEAD, MAX_CLOCK_SHARE_DIVISOR and the .max(1) floor exactly as they are, so TASK-7 overflow safety and TASK-32 zero-budget behavior are unaffected.
4. Add a full-game simulation test over 1+0.01, 2+0.05 and 10+0.1 asserting the clock at moves 60, 100 and 140 stays above the reserve, plus a late-game test showing an allocation materially above the increment when the clock is above the reserve.
5. Update the two exact-value assertions in the TASK-38 opening test to the new policy's numbers and re-point increment_contributes_to_allocation at a clock above the reserve; the properties they encode are preserved.
6. Run cargo fmt --check, strict clippy, cargo test --workspace, then a FastChess self-play match against the pre-change build at 1+0.01 and 2+0.05 for AC5.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reserve policy: hold back RESERVE_INCREMENT_MOVES (10) moves' worth of increment, enforced as a
cap on how fast the clock may drain rather than as a deduction from the pool being divided.
Spending `inc + x` and earning `inc` back drains the clock by exactly `x`, so bounding `x` by the
headroom above the reserve states precisely that this move will not take us below it.

Converged clock (full-game simulation, allocation policy only):
  1+0.01   49ms -> 130ms
  2+0.05   96ms -> 530ms
  10+0.1  163ms -> 1030ms

Every allocation above the reserve is bit-for-bit unchanged, so the TASK-38 opening values (100ms
at 2+0.05, 34ms at 1+0.01) and the TASK-7 overflow expectation stand unmodified; engine/src/uci.rs
is untouched. The diff is confined to engine/src/time.rs.

A first attempt deducted the reserve up front instead. That paid for the reserve in the opening and
midgame, where the clock is nowhere near it, and measured -7.9 Elo over 1711 games. Superseded.

AC5 strength evidence (1+0.01, authoritative, 2000 games, target 2cc4d1c vs base 9b7bf33):
  verdict INCONCLUSIVE; SPRT LLR -0.77 against bounds +/-2.94
  Elo -6.60 +/- 8.45 (95% CI [-15.1, +1.9]); pentanomial [39, 154, 669, 82, 56]
  0 time forfeits, 0 crashes, 0 illegal moves
  depth-1 moves after move 60: baseline 3 of 21448, candidate 0 of 21356
  artifacts: ~/seaborg-strength-artifacts/task-42-1plus001-2cc4d1c
  binaries sha256 base 32cf93fb..., candidate 03b94392...

AC5 is NOT met as written and is not checked here.
- "Non-negative Elo delta" is not demonstrated. The point estimate is negative, the interval spans
  zero, and the SPRT is INCONCLUSIVE, which docs/strength-testing.md says is never a pass.
- The cost is intrinsic, not an implementation defect. Between roughly move 45 and 100 at 1+0.01
  the base build spends 13ms per move where this build spends 10ms, because it eats the time the
  reserve now protects. Any reserve worth holding is a decision not to spend time the old policy
  spent, and self-play over local pipes is exactly the environment where the reserve's benefit
  (GUI latency, machine load, search overshoot) cannot appear. Two independent designs measured
  the same direction.
- "A reduction in depth-1 moves played after move 60" is close to unmeasurable: the symptom the
  task cites from the TASK-38 review (5 of 5569) is essentially gone from the base build.
- 2+0.05 was not run. The human directed stopping once the trade was understood.

Tooling defect found while reading the evidence: tools/strength/strength_test.py writes FastChess's
nElo into report.json's `elo` field. This run's report.json says elo -11.91 +/- 15.23; the runner
log says Elo -6.60 +/- 8.45, nElo -11.91 +/- 15.23. A merge gate overstating its headline
regression metric by ~1.8x is worth fixing. No follow-up task created; that is the human's call.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 12:51
---
Implementation handoff
Branch: task-42-increment-time-reserve
Worktree: /Users/seabo/seaborg-worktrees/task-42-increment-time-reserve
Base: 9b7bf3392ccd4adf43effdaa990bacb45c40a15c
Implementation target: 2cc4d1c
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (209 engine, 43 core, 5 build metadata, 1 integration; 3 ignored)
- strength test 1+0.01, 2000 games, authoritative: INCONCLUSIVE, Elo -6.60 +/- 8.45, 0 forfeits
Known failures: none

Reviewer note: AC5 is not met as written and is deliberately left unchecked. The Elo delta is
negative with an interval spanning zero and the SPRT is INCONCLUSIVE, which docs/strength-testing.md
states is never a pass. The implementation notes argue this cost is intrinsic to holding any
reserve rather than a defect in this implementation, and that the depth-1 clause is no longer
measurable against the current base build. The human was shown this evidence and directed landing
the change on that basis; AC5 needs amending by a human rather than ticking. AC1-AC4 are supported
by unit tests in engine/src/time.rs and by the untouched TASK-7/32/38 fixtures.
---
<!-- COMMENTS:END -->
