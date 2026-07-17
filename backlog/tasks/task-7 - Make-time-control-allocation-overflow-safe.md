---
id: TASK-7
title: Make time-control allocation overflow safe
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 18:55'
labels:
  - search
  - uci
  - time
dependencies: []
references:
  - engine/src/time.rs
  - engine/src/engine.rs
priority: high
type: bug
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ordinary timed searches can panic after move 40 or underflow when the per-move allocation is below the safety buffer. Time allocation must remain bounded and meaningful for late games, short clocks, increments, and moves-to-go controls.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Timed search allocation does not panic or wrap for move numbers above the average game length
- [ ] #2 Allocations at very low remaining time saturate safely instead of underflowing
- [ ] #3 Large protocol time values are handled without lossy narrowing
- [ ] #4 Tests cover late-game move numbers, sub-buffer clocks, increments, and explicit moves-to-go values
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Represent UCI clock, increment, moves-to-go, and move-time values with explicit u64 widths through parsing and search-limit conversion.
2. Make estimated-move and allocation arithmetic saturating and guard zero moves-to-go while preserving a minimum late-game horizon.
3. Add focused unit tests for late games, sub-buffer clocks, increments, explicit moves-to-go, and values above u32::MAX.
4. Run formatting and the full Rust workspace test suite, commit the implementation, and record an immutable review handoff.
<!-- SECTION:PLAN:END -->
