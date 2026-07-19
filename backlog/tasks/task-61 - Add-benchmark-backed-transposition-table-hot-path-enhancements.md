---
id: TASK-61
title: Add benchmark-backed transposition-table hot-path enhancements
status: To Do
assignee: []
created_date: '2026-07-19 00:01'
labels:
  - transposition-table
  - performance
  - search
  - benchmark
dependencies:
  - TASK-59
  - TASK-60
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: medium
type: enhancement
ordinal: 60000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After correctness, replacement, and quiescence integration are stable, evaluate the remaining common TT hot-path opportunities rather than adopting them on folklore alone. The principal candidates are storing a position’s static evaluation to avoid duplicate work and support pruning, and prefetching the child bucket before recursive search. Coordinate with TASK-50, TASK-51, and TASK-52 so metadata supports forthcoming pruning without coupling this task to those search changes. TASK-43 separately owns TT-assisted PV extension.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Representative fixed-depth positions and a reproducible benchmark establish baseline nodes, elapsed time, and probe behavior before hot-path changes
- [ ] #2 The value and validity conditions for a stored static evaluation are specified, including interaction with rule-sensitive evaluation from TASK-58; it is implemented only if measurements or imminent pruning consumers justify its entry-space cost
- [ ] #3 Child-bucket prefetching is evaluated on supported targets and retained only if it produces a repeatable benefit without harming portability or safety
- [ ] #4 Accepted enhancements include regression and benchmark coverage; rejected candidates have their measurements and decision recorded so the experiment is not repeatedly rediscovered
- [ ] #5 The final entry layout remains compact and its memory footprint and cache-line organization are asserted or tested
<!-- AC:END -->
