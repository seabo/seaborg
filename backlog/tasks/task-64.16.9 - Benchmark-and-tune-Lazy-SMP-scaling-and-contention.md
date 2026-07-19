---
id: TASK-64.16.9
title: Benchmark and tune Lazy SMP scaling and contention
status: To Do
assignee: []
created_date: '2026-07-19 23:25'
labels:
  - search
  - concurrency
  - performance
  - benchmark
dependencies:
  - TASK-64.16.7
references:
  - engine/src/search.rs
  - engine/src/tt.rs
  - engine/src/trace.rs
  - benches/search.rs
  - docs/strength-testing.md
parent_task_id: TASK-64.16
priority: medium
type: enhancement
ordinal: 100000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Measure and improve how efficiently the production Lazy SMP search converts additional CPU resources into useful search. Functional multi-worker search can still scale poorly because of TT cluster contention, false sharing in shared control data, counter publication, allocator traffic, oversized per-worker state, SMT interference, or redundant tree exploration.

Profile before changing architecture. Establish representative single-position and game-search workloads, report both raw NPS and useful search outcomes such as depth and effective branching, and separate algorithmic effects from per-node cost. Thread affinity, NUMA placement, TT sharding, or platform-specific code are not assumed solutions: pursue them only when evidence identifies a material bottleneck. Permanent TT partitioning is incompatible with the worker-agnostic shared-table contract unless a later task explicitly revises that architecture with strength evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Benchmarks record wall time, total nodes, NPS, completed depth, TT probe outcomes, hashfull, and scaling efficiency at one, two, four, eight, and a documented higher worker count
- [ ] #2 Measurements cover multiple hash sizes and distinguish physical-core from SMT scaling on the test hardware
- [ ] #3 Profiles assess TT cluster contention, false sharing, shared-counter traffic, event publication, allocation, worker startup, and per-worker memory and stack use
- [ ] #4 Shared control fields and frequently written worker fields do not create avoidable false sharing, with layout evidence recorded where alignment is introduced
- [ ] #5 Threads=1 throughput remains within a documented non-regression threshold relative to the pre-SMP baseline
- [ ] #6 Every retained optimization has before-and-after scaling evidence and preserves correctness and strength invariants
- [ ] #7 Affinity, NUMA-aware allocation, persistent workers, or other platform complexity is added only when a measured bottleneck and portable fallback are documented
- [ ] #8 Recommended thread and hash sizing guidance is recorded for normal play and high-throughput self-play
<!-- AC:END -->
