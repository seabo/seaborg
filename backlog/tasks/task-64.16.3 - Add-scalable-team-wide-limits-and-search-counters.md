---
id: TASK-64.16.3
title: Add scalable team-wide limits and search counters
status: To Do
assignee: []
created_date: '2026-07-19 23:23'
labels:
  - search
  - concurrency
  - performance
  - uci
dependencies:
  - TASK-64.16.2
  - TASK-64.6
references:
  - engine/src/search.rs
  - engine/src/trace.rs
  - engine/src/time.rs
  - engine/src/info.rs
parent_task_id: TASK-64.16
priority: high
type: feature
ordinal: 94000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Give a search team one coherent control plane for cancellation, absolute deadlines, node budgets, and aggregate statistics. The current Search stores a local Tracer and local node limit, so naively cloning it would make go nodes N spend N nodes per worker and UCI would report only the master. Conversely, a shared atomic increment at every node would put a contended read-modify-write in the hottest path.

Design a scalable accounting scheme based on worker-local counters plus bounded periodic publication or reservation. The observable semantics and maximum overshoot must be documented. Explicit cancellation stays cheap and prompt on every worker; clock sampling remains throttled. A single root search has one deadline and one TT age even though it has several workers.

This task supplies team control and accounting primitives. Formatting the complete UCI event stream belongs to the later reporting task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All workers observe one cancellation state and one absolute hard deadline, and explicit cancellation stops the complete team promptly
- [ ] #2 go nodes N is enforced as a team-wide budget rather than N nodes per worker, with a documented and tested maximum overshoot
- [ ] #3 Node-budget enforcement and progress publication avoid a contended atomic read-modify-write on every searched or quiescence node
- [ ] #4 Team-wide main nodes, quiescence nodes, elapsed time, and derived NPS can be obtained without reading mutable worker state unsafely
- [ ] #5 The guaranteed legal fallback and first-ply policy is defined for the team and cannot multiply the guaranteed work silently by the worker count
- [ ] #6 Counters have defined overflow behavior on supported targets and preserve the exact reproducible one-worker node-limit behavior
- [ ] #7 Tests cover zero, tiny, boundary, and large node budgets at one, two, and eight workers plus cancellation and expired deadlines
- [ ] #8 Hot-path benchmarks show that the accounting design does not materially regress Threads=1 node throughput
<!-- AC:END -->
