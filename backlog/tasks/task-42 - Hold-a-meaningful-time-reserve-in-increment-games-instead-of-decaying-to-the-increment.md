---
id: TASK-42
title: >-
  Hold a meaningful time reserve in increment games instead of decaying to the
  increment
status: To Do
assignee: []
created_date: '2026-07-18 13:18'
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
