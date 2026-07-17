---
id: TASK-7
title: Make time-control allocation overflow safe
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
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
