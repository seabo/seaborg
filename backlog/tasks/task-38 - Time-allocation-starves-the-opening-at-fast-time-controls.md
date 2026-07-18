---
id: TASK-38
title: Time allocation starves the opening at fast time controls
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 11:45'
updated_date: '2026-07-18 12:10'
labels:
  - engine
  - time
  - search
dependencies:
  - TASK-32
references:
  - engine/src/time.rs
priority: high
type: bug
ordinal: 38000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TimeControl::to_move_time (engine/src/time.rs:43) computes base_time / est_remaining_moves, adds the increment, then subtracts a flat PER_MOVE_BUFFER_TIME of 150ms and saturates at zero. At fast time controls the buffer dominates the whole allocation, so the engine is handed a 0ms budget and plays instantly at depth 1.

Worked example at tc=2+0.05 (wtime=2000, inc=50). With AVERAGE_GAME_LENGTH=40 and MINIMUM_REMAINING_MOVES=20, est_remaining_moves is 20-40, so base_time_per_move is 50-100ms. Adding the 50ms increment gives 100-150ms, and subtracting the 150ms buffer saturates to 0.

Observed in FastChess self-play on the TASK-32 branch (40 games at tc=2+0.05). Roughly the first 13 moves of every game are played at depth 1 in 0.000s. Only once the unspent increment has banked enough clock does base_time/est_remaining_moves exceed the buffer and the engine start searching for ~45ms per move. A representative PGN comment sequence: 1040 moves annotated {0.00/1 0.000s} across the match, then a ramp through {0.00/3 0.003s}, {0.00/4 0.009s}, {0.00/5 0.016s} to a steady {+4.00/5 0.045s}.

This is a strength defect, not a legality or forfeit defect. It is present on master and predates TASK-32. TASK-32 makes a zero budget survivable by guaranteeing one completed ply, so the engine no longer forfeits, but it does not change allocation: the engine still throws away the opening and finishes a 2-second game having spent a small fraction of its clock. Fixing this is what makes timed self-play strength-meaningful rather than merely legal, so it gates authoritative use of the TASK-27 strength-regression tooling at fast time controls.

The buffer is also the wrong shape. A flat 150ms reserve is a fixed communication/scheduling safety margin, but it is being subtracted from a per-move slice rather than from the clock as a whole, so its relative cost grows without bound as the time control shortens. Any fix should keep a genuine safety margin against flagging while ensuring the allocation degrades proportionally instead of collapsing to zero.

Do not regress TASK-7 (allocation overflow safety) or TASK-32 (guaranteed legal move at any budget); both must continue to hold.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At fast time controls (2+0.05 and faster) the per-move allocation is a positive, proportional fraction of the remaining clock rather than saturating to zero, and the engine no longer plays its opening moves at depth 1 in 0.000s
- [ ] #2 A safety margin against flagging is retained: self-play at 2+0.05, 10+0.1, and at least one faster control (for example 1+0.01) produces zero losses on time across a multi-game match
- [ ] #3 Allocation degrades proportionally as the time control shortens instead of collapsing once a flat buffer exceeds the per-move slice, verified by unit tests over a range of clock/increment/moves-to-go combinations including very small clocks
- [ ] #4 TASK-7 overflow safety and TASK-32 guaranteed-legal-move behavior still hold, evidenced by their existing regression tests continuing to pass
- [ ] #5 A FastChess self-play match at 2+0.05 shows the engine using a materially larger share of its clock than before (reported depth and time per move rise above depth 1 / 0.000s from the opening onward), with zero illegal moves and zero time forfeits
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Investigate the full time-management subsystem and confirm the defect is confined to the allocation policy in TimeControl::to_move_time rather than the uci -> engine -> search pipeline around it.
2. Rewrite to_move_time with a proportional policy: subtract a fixed MOVE_OVERHEAD from the remaining clock once (not from the per-move slice), divide the usable clock by the moves-to-go estimate, add the increment, then clamp the result to a fraction of the usable clock so allocation can never exceed what is actually on the clock.
3. Keep all arithmetic u64 and saturating, and express the clamp so it cannot overflow for very large clocks, preserving TASK-7.
4. Update the existing tests that encode the old behavior (sub_buffer_allocation_saturates_at_zero asserts the bug as correct; the buffer-subtraction expectations in the other cases change), and add a test matrix covering proportional degradation across clock/increment/moves-to-go combinations including very small clocks, plus an explicit never-exceeds-the-clock test.
5. Verify TASK-32 search regression tests still pass unchanged; allocation may now be small but positive rather than zero, and the guaranteed-ply behavior must be untouched.
6. Run cargo fmt, clippy and the full test suite.
7. Run FastChess self-play matches at 2+0.05, 10+0.1 and 1+0.01, recording per-move depth and time, illegal-move count and time-forfeit count as objective evidence for AC #2 and AC #5, and compare the 2+0.05 result against the pre-fix baseline.
8. File a follow-up ticket for the out-of-scope search-side time work: no soft/hard limit split, no prediction of whether the next deepening iteration fits the budget, and Instant::now() called per node in stopping() with no node-count throttle.
9. Commit the implementation, append notes, and hand off to review.
<!-- SECTION:PLAN:END -->
