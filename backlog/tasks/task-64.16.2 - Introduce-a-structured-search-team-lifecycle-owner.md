---
id: TASK-64.16.2
title: Introduce a structured search-team lifecycle owner
status: To Do
assignee: []
created_date: '2026-07-19 23:23'
labels:
  - search
  - concurrency
  - lifecycle
dependencies:
  - TASK-64.16.1
references:
  - engine/src/search.rs
  - engine/src/engine.rs
  - engine/src/game.rs
  - engine/src/tt.rs
parent_task_id: TASK-64.16
priority: high
type: feature
ordinal: 93000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Generalize SearchHandle from ownership of one worker into structured ownership of a complete search team. The owner must make it impossible for a master or helper to detach, survive a dropped handle, or retain the shared table after the team is reported finished.

This task establishes orchestration and failure containment, not playing-strength diversification. It should support one master plus a configured number of homogeneous helpers, but production UCI advertisement may remain gated until the baseline multi-worker task is complete. Thread creation is fallible: a partial spawn must not panic after leaving already-created workers running. A worker panic must cancel its siblings, release resources, and produce a defined team outcome rather than deadlock the driver.

The implementor must evaluate per-search spawning against a persistent worker pool. Per-search workers naturally drop their Arc<Table> clones at quiescence; a persistent pool must not retain the old table across new-game clearing or hash resizing. Choose the simpler model unless benchmarks demonstrate material benefit from persistence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One lifecycle owner contains every master and helper handle and provides cancel, wait, completion observation, and join-on-drop for the whole team
- [ ] #2 Completion is signalled exactly once and only after every started worker has exited and released its search resources
- [ ] #3 Dropping the team owner cancels and joins every worker without propagating a worker panic during unwinding
- [ ] #4 A partial thread-spawn failure cancels and joins already-started workers and returns a controlled failure without detaching work or corrupting the next search
- [ ] #5 A worker panic cancels the remaining team, cannot deadlock the driver, and is surfaced through a documented team outcome or process-failure policy
- [ ] #6 The selected per-search or persistent-worker model is justified with startup-cost measurements and preserves the exclusive TT clear and resize boundary
- [ ] #7 Focused tests cover one worker, several workers, drop without wait, repeated cancel, partial spawn failure, a panicking helper, and a slow final helper
<!-- AC:END -->
