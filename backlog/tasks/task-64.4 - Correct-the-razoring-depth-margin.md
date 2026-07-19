---
id: TASK-64.4
title: Correct the razoring depth margin
status: To Do
assignee: []
created_date: '2026-07-19 13:31'
labels:
  - search
  - pruning
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: bug
ordinal: 67000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The razoring margin grows with the square of depth, which makes the technique inert at every depth it is nominally enabled for.

`should_razor` (search.rs:31-33) is:

    depth <= 6 && alpha.is_cp() && eval + Score::cp(426 + 252 * depth as i16 * depth as i16) < alpha

The margin is 678cp at depth 1, 1434cp at depth 2, 2694cp at depth 3, and 9498cp at depth 6. A margin above roughly a queen means the guard can essentially only fire in positions already decided by material, so razoring is doing no work at depths 2 through 6 despite the depth <= 6 gate advertising otherwise. The shape strongly suggests `252 * depth` was intended and the second factor is a transcription error; the linear form is what the conventional formulation uses.

Note that the razoring call at search.rs:769 also hardcodes the thread type as `Master` (`self.quiesce::<Master, NonPv>`) rather than propagating the caller's `T`. This is harmless today because quiescence emits no events, but it stops being harmless once worker threads exist, and it should be corrected while this code is being touched.

Correcting the margin will change how often razoring fires from almost never to routinely, so this is a behavioural change that needs measuring rather than a typo fix that can be asserted correct by inspection. If the corrected form measures worse than the inert form, that is a meaningful result about the current evaluation and should be recorded: razoring compares a static evaluation against a margin, and the evaluation is material-only.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The razoring margin scales linearly with depth, or an alternative form is adopted with recorded rationale
- [ ] #2 The thread type parameter is propagated to the quiescence call rather than hardcoded to Master
- [ ] #3 A test asserts razoring fires at a mid-range depth for a position whose evaluation is clearly below alpha, which the current squared margin would not
- [ ] #4 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes, including the outcome if the corrected form measures no better
<!-- AC:END -->
