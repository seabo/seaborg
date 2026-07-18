---
id: TASK-41
title: >-
  Throttle the clock read in Search::stopping() to avoid a per-node
  Instant::now()
status: To Do
assignee: []
created_date: '2026-07-18 12:17'
labels:
  - engine
  - search
  - performance
dependencies: []
priority: medium
type: bug
ordinal: 41000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search::stopping() (engine/src/search.rs:767-778) calls std::time::Instant::now() every time it is invoked, and it is invoked on the hot path: the iterative-deepening loop (search.rs:446, 454), the main search entry (search.rs:491), the interior move loop (search.rs:630), post-loop (search.rs:715), quiescence entry (search.rs:812), the quiescence move loop (search.rs:898) and the evasions loop (search.rs:931). That is roughly once per node and once per move, with no throttle.

Engines conventionally sample the clock only every N nodes (a mask test such as 'nodes & 4095 == 0') precisely because this read is not free. On macOS Instant::now() is a mach_absolute_time call; it is cheap relative to a syscall but not relative to a node, and it sits inside the innermost loops of the search.

The cost has not been measured in this repo, so the first job is to measure it rather than assume it. If it is material, throttle the clock read behind a node-count check while keeping the cancellation-flag read unthrottled (an atomic bool load is genuinely cheap, and TASK-39 is separately concerned with how responsive 'stop' is, so the flag should not become less responsive as a side effect).

Note the interaction with the TASK-32 guarantee: stopping() returns false outright until min_search_complete, so any throttle must not disturb that early return. Note also engine/src/trace.rs:141, which divides by elapsed microseconds and will divide by zero on a sufficiently fast search; that is adjacent and cheap to fix here if touched.

Identified during the TASK-38 investigation and deliberately left out of that ticket's scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The per-node cost of the clock read is measured on a representative search and the figure recorded in the task, establishing whether a throttle is warranted
- [ ] #2 If warranted, the deadline check is sampled on a node-count interval rather than on every stopping() call, and the cancellation-flag check remains unthrottled
- [ ] #3 The search still respects its time budget within a documented tolerance, verified by a test asserting actual elapsed wall time against the budget
- [ ] #4 TASK-32 guaranteed-legal-move behavior is unaffected, evidenced by its existing regression tests passing
- [ ] #5 A before/after benchmark on the existing hot-path benchmarks shows no regression, and the nps change is reported
<!-- AC:END -->
