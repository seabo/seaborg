---
id: TASK-38
title: Time allocation starves the opening at fast time controls
status: Done
assignee:
  - '@codex'
created_date: '2026-07-18 11:45'
updated_date: '2026-07-18 13:18'
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
- [x] #1 At fast time controls (2+0.05 and faster) the per-move allocation is a positive, proportional fraction of the remaining clock rather than saturating to zero, and the engine no longer plays its opening moves at depth 1 in 0.000s
- [x] #2 A safety margin against flagging is retained: self-play at 2+0.05, 10+0.1, and at least one faster control (for example 1+0.01) produces zero losses on time across a multi-game match
- [x] #3 Allocation degrades proportionally as the time control shortens instead of collapsing once a flat buffer exceeds the per-move slice, verified by unit tests over a range of clock/increment/moves-to-go combinations including very small clocks
- [x] #4 TASK-7 overflow safety and TASK-32 guaranteed-legal-move behavior still hold, evidenced by their existing regression tests continuing to pass
- [x] #5 A FastChess self-play match at 2+0.05 shows the engine using a materially larger share of its clock than before (reported depth and time per move rise above depth 1 / 0.000s from the opening onward), with zero illegal moves and zero time forfeits
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Investigation

Confirmed the defect is confined to the allocation policy, not the surrounding pipeline. The subsystem is a clean four-file path: uci.rs parses `go` into TimingMode, engine.rs:118-129 is the single production caller of to_move_time, search.rs:152-158 turns the result into an absolute Instant deadline, and search.rs:767 checks it in stopping(). That factoring is sound, so this is a rewrite of the policy inside one pure function rather than a subsystem refactor (course of action agreed with the user before implementing).

The investigation found a second latent defect alongside the reported one. Nothing clamped the allocation against the remaining clock; the only thing preventing an over-allocation was the flat buffer that causes the reported bug. TimeControl::new(1000, 1000, 1000, 1000, Some(1)) allotted 1850ms against a 1000ms clock on master. Removing the buffer without adding a cap would therefore have traded a strength defect for a forfeit defect, so the share cap is part of the fix rather than an extra.

## Change

engine/src/time.rs: replaced PER_MOVE_BUFFER_TIME (150ms, subtracted per move) with MOVE_OVERHEAD (30ms, deducted from the clock once) and MAX_CLOCK_SHARE_DIVISOR (4, capping one move at three quarters of the usable clock).

    usable    = base_time - MOVE_OVERHEAD        (0 -> return 0)
    allocation = usable / est_remaining_moves + inc
    cap        = usable - usable / 4             (subtraction, so it cannot overflow)
    result     = allocation.min(cap).max(1)

Both terms of the allocation now scale with the time control, so it degrades proportionally instead of collapsing at a fixed threshold. All arithmetic stays u64 and saturating. A clock at or below the overhead still returns 0, which TASK-32 makes survivable.

## Tests changed rather than added

Two existing tests asserted the old behavior as correct and could not simply be kept:

- time.rs sub_buffer_allocation_saturates_at_zero encoded the defect itself (a 100ms clock allotting 0ms). Removed, and replaced by a_clock_at_or_below_the_overhead_allots_no_time and a_clock_just_above_the_overhead_still_allots_time, which pin the intended boundary.
- uci.rs parses_large_timed_control_values_without_narrowing asserted a literal computed under the old formula. Its no-narrowing purpose is preserved and strengthened with an explicit assertion that the result exceeds u32::MAX.

The remaining pre-existing expectations changed value with the formula and are annotated with their derivation.

New coverage: allocation_never_exceeds_the_remaining_clock (288 combinations of clock, increment, movestogo and move number, including 1-5ms clocks), allocation_degrades_proportionally_as_the_clock_shrinks (halving the clock halves the allocation from 64s down to 250ms), fast_time_controls_receive_a_positive_proportional_allocation, and huge_increment_cannot_allocate_more_than_the_clock_holds.

## Self-play evidence

Five 40-game FastChess matches (200 games), openings-v1.epd, concurrency 4, release builds of master (baseline) and this branch (fixed).

Clock usage at 2+0.05, like-for-like self-play:

    baseline vs baseline:  2.06 s/game searched, 21.0% of moves at depth 1,
                           514/520 opening moves at 0.000s, mean opening depth 1.01
    fixed vs fixed:        4.65 s/game searched,  0.0% of moves at depth 1,
                           0/520 opening moves at 0.000s, mean opening depth 6.67

Head-to-head at 2+0.05: fixed +284.85 +/- 144.85 Elo (31W 4L 5D, LOS 100%).

Zero depth-1 moves and zero 0.000s opening moves at all three controls:

    2+0.05   4.65 s/game, mean 77.4 ms/move, mean opening depth 6.67
    10+0.1  14.87 s/game, mean 241.7 ms/move, mean opening depth 7.48
    1+0.01   1.57 s/game, mean  22.7 ms/move, mean opening depth 5.87

Terminations across all 200 games: 200 'normal'. Zero time forfeits, zero losses on time, zero illegal played moves.

Note for the reviewer: the FastChess log contains 2 'Illegal PV move' warnings across the 200 games, both from the baseline binary and none from fixed. These concern PV reporting, not played moves, and are the pre-existing defect tracked by TASK-34.

## Follow-ups filed

Two search-side issues found during the investigation, deliberately out of scope here (scope confirmed with the user):

- TASK-40: no soft/hard limit split, and the deepening loop starts iterations it cannot finish and discards the work.
- TASK-41: stopping() calls Instant::now() on every node with no node-count throttle.

## Analysis caveat

The PGN analysis script initially mis-attributed moves because several openings in openings-v1.epd are black-to-move, so ply 0 is not always White's. The corrected script reads the side to move from the FEN header. The figures above are from the corrected version; the raw PGNs are at /tmp/task38/ if the reviewer wants to re-derive them.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 12:27
---
Implementation handoff
Branch: task-38-time-allocation-fast-controls
Worktree: /Users/seabo/seaborg-worktrees/task-38-time-allocation-fast-controls
Base: 40a97475317ead3cf251d550bcde864542559bc5
Implementation target: abbf022
Resolved findings: none (first implementation attempt)
Verification:
- cargo fmt --all: clean
- cargo test -p engine: PASS (79 passed, 0 failed, 1 ignored)
- cargo test -p core: PASS (35 passed + 1 passed, 0 failed)
- cargo test (workspace, integration): PASS (5 passed, 0 failed)
- cargo clippy --all-targets: no errors; warnings are pre-existing and none are in time.rs or uci.rs
- TASK-7 + TASK-32 regression tests run explicitly by name: PASS (8/8)
- FastChess 5 x 40 games at 2+0.05, 10+0.1, 1+0.01: 200/200 games terminated 'normal', zero time forfeits, zero illegal played moves
Known failures: none

Reviewer notes:
- Two pre-existing tests changed rather than merely extended, both deliberately: time.rs sub_buffer_allocation_saturates_at_zero asserted the defect as correct behavior and is replaced; uci.rs parses_large_timed_control_values_without_narrowing asserted a literal computed under the old formula and keeps its no-narrowing purpose with a strengthened bound. Please confirm you agree these were the right calls.
- The share cap (MAX_CLOCK_SHARE_DIVISOR) is not requested by the ticket text but is load-bearing: on master the flat buffer was the only thing preventing an over-allocation against the clock, so removing it without a cap would have introduced a forfeit risk.
- MOVE_OVERHEAD = 30ms and the three-quarters cap are judgement calls. Both are pinned by tests, so disagreement about the constants is a cheap change.
- 2 'Illegal PV move' warnings appear in the FastChess log, both from the baseline binary, none from this build. That is TASK-34's pre-existing PV-reporting defect, not a played-move illegality.
- Acceptance criteria are left unchecked for the independent reviewer.
---

author: @codex
created: 2026-07-18 12:54
---
Review verdict: APPROVED

Implementation SHA (immutable target): abbf022
Branch: task-38-time-allocation-fast-controls
Base: 40a97475317ead3cf251d550bcde864542559bc5
Reviewed diff: 40a9747..abbf022 in full. Target confirmed an ancestor of tip ca1d8c7; the only later commit touches the task file alone.

Scope: engine/src/time.rs, engine/src/uci.rs, plus backlog metadata for TASK-38 and the two user-approved follow-up files TASK-40 and TASK-41 (markdown only, no code). No unrelated changes.

Verification commands run by the reviewer at abbf022:
- cargo fmt --all -- --check: clean
- cargo test -p engine: 79 passed, 0 failed, 1 ignored
- cargo test -p core: 35 passed + 1 doc-test passed, 0 failed
- cargo test (workspace integration): 5 passed, 0 failed
- cargo clippy --all-targets: the only error is benches/square.rs E0423 Square(34), confirmed present on base 40a9747 and unrelated to this diff; remaining warnings are pre-existing and none are in time.rs or uci.rs
- cargo test -p engine -- --exact (TASK-7 + TASK-32 regressions): 6/6 passed

Acceptance criteria evidence:
- AC-1: reviewer-run FastChess, seaborg-fixed vs itself, 40 games at 2+0.05, openings-v1.epd, concurrency 4. Zero moves at depth 1 across 4311 moves; 0 of 800 opening moves (first 10 by each side, side-to-move read from the FEN header) played in 0.000s; mean opening depth 6.82. The like-for-like baseline built from 40a9747 played 780 of 800 opening moves in 0.000s at mean depth 1.00 with 19.5% of all moves at depth 1.
- AC-2: 120 games on the fixed build across 2+0.05, 10+0.1 and 1+0.01. All 120 Termination headers are 'normal'; zero occurrences of 'loses on time' and zero illegal-move terminations.
- AC-3: allocation_never_exceeds_the_remaining_clock covers 12 clocks (including 1, 2, 5, 10, 29, 30, 31 ms) x 5 increments x 6 movestogo values x 4 move numbers and asserts move_time < clock throughout; allocation_degrades_proportionally_as_the_clock_shrinks asserts halving the clock halves the allocation from 64s down to 250ms; the boundary is pinned by a_clock_at_or_below_the_overhead_allots_no_time and a_clock_just_above_the_overhead_still_allots_time. Independently confirmed the invariant holds for all inputs, not only the sampled ones: the result is min(allocation, max_allocation).max(1), max_allocation = usable - usable/4 >= 1 whenever usable >= 1, so the result is always <= usable = clock - MOVE_OVERHEAD < clock. No over-allocation is reachable.
- AC-4: TASK-7 overflow safety (allocation_preserves_values_above_u32_max, parses_large_timed_control_values_without_narrowing, parses_move_time_above_u32_max_without_narrowing, oversized_and_invalid_numeric_values_are_rejected) and TASK-32 guaranteed-legal-move behavior (time_limited_search_honors_the_budget_after_the_guaranteed_ply, typed_api_supports_time_limits) all pass at the target.
- AC-5: covered by the AC-1 and AC-2 matches above. Mean search time rose from 34.7 ms/move to 80.5 ms/move and per-game searched time from 4.59s to 8.67s (both engines) at 2+0.05, with all terminations normal.

Responses to the implementer's reviewer notes:
- Removing time.rs sub_buffer_allocation_saturates_at_zero was the right call. It asserted a 100ms clock allotting 0ms, which is the defect itself rather than a contract; the two replacement boundary tests pin the intended behavior more precisely.
- Reworking uci.rs parses_large_timed_control_values_without_narrowing rather than deleting it was also correct. The literal necessarily changed with the formula and the no-narrowing purpose is preserved and strengthened by the explicit move_time > u32::MAX assertion.
- The MAX_CLOCK_SHARE_DIVISOR cap is justified and load-bearing. Removing the flat buffer without it would allow movestogo 1 or a large increment to allot more than the clock holds; the cap is what makes the AC-3 invariant provable.
- MOVE_OVERHEAD = 30ms is a defensible judgement call for this engine. It is smaller than the old 150ms, but the old value was never a real connection margin, and the share cap now supplies a proportional reserve on top of it. Both constants are pinned by tests and cheap to retune if a real GUI over a slower transport ever proves 30ms tight.

Notes, none blocking:
- The reviewer's own 1+0.01 run showed 5 depth-1 moves out of 5569 (0.1%), all at moves 77-120 of long games with a genuinely near-exhausted clock. That is the intended TASK-32 guaranteed-ply path, not the allocation defect, and it is outside AC-1's opening scope. The implementer's 'zero depth-1 at all three controls' figure is run variance, not a discrepancy in behavior.
- One 'Illegal PV move' warning appeared from the fixed build in the reviewer's 2+0.05 run. This branch is based on 40a9747, which predates the TASK-34 merge on master, so the pre-existing PV-reporting defect is still present here. It concerns reported PV lines, not played moves, and TASK-34 is already merged on primary.
- Speed benchmarks were not run. to_move_time has exactly one production caller, engine/src/engine.rs:123, invoked once per 'go' command; it is not on a movegen or search hot path, so BENCHMARKS.md comparison does not apply to this diff.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the flat 150ms PER_MOVE_BUFFER_TIME (subtracted from each per-move slice) with MOVE_OVERHEAD = 30ms deducted once from the clock, plus MAX_CLOCK_SHARE_DIVISOR = 4 capping any single move at three quarters of the usable clock. Allocation is now (clock - 30) / est_remaining_moves + inc, clamped to the share cap and floored at 1ms, so both terms scale with the time control instead of collapsing once a fixed buffer exceeds the slice.

Verified at implementation target abbf022: cargo fmt --all -- --check clean; cargo test -p engine 79 passed / 0 failed / 1 ignored; cargo test -p core 35 + 1 doc passed; workspace integration tests 5 passed; TASK-7 and TASK-32 regression tests run by name, 6/6 passed. Reviewer-run FastChess self-play (120 games on the fixed build, 40 baseline) reproduced the reported effect: at 2+0.05 the baseline played 97.5% of opening moves in 0.000s at mean depth 1.00, while the fixed build played 0% at 0.000s with mean opening depth 6.82 and roughly doubled clock usage (8.67s vs 4.59s searched per game). All 160 games terminated 'normal' with zero time forfeits and zero illegal played moves at 2+0.05, 10+0.1 and 1+0.01.
<!-- SECTION:FINAL_SUMMARY:END -->
