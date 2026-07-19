---
id: TASK-64.16.1
title: Specify the Lazy SMP search-team contract
status: To Do
assignee: []
created_date: '2026-07-19 23:23'
labels:
  - search
  - concurrency
  - architecture
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/tt.rs
  - engine/src/engine.rs
  - engine/src/game.rs
  - backlog/tasks/task-35
  - backlog/tasks/task-45
  - backlog/tasks/task-46
  - backlog/tasks/task-57
parent_task_id: TASK-64.16
priority: high
type: task
ordinal: 92000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define the durable behavioral and ownership contract for a multi-worker root search before worker orchestration is implemented. The contract must make all later tasks independently implementable without relying on implicit assumptions about which state is shared, which result is authoritative, or when a team is considered stopped.

The current engine has one SearchHandle containing one JoinHandle, one cancellation token, an explicit completion receiver, and one master event stream. Search owns the mutable board and heuristic state; SearchEngine owns an Arc<Table>. The contract must generalize these boundaries while preserving TASK-35 explicit completion, TASK-45 prompt cancellation after a legal fallback, TASK-46 aborted-subtree semantics, TASK-57 TT sharing and quiescent clearing, and the join-on-drop guarantee.

This task records architecture and tests the seams needed to enforce it; it does not spawn multiple production workers. It must distinguish baseline correctness policy from later strength experiments: initially the master last completed iteration is authoritative, while helpers influence it only through the TT. Cross-worker voting belongs to a later task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A checked-in module-level contract defines one master, zero or more helpers, team identity, shared state, per-worker state, and the authoritative-result rule
- [ ] #2 The contract defines completed, cancelled, failed, and panicked team outcomes and when the single explicit completion signal may be emitted
- [ ] #3 The contract specifies fixed-depth, time, node, and infinite limit semantics for the complete team, including which worker decides normal completion
- [ ] #4 The contract states that TT age advances once per root search, every worker shares one table allocation, and clearing or replacement occurs only after all workers release it
- [ ] #5 The contract preserves legal root fallback and prohibits partial or aborted iterations from becoming official results
- [ ] #6 Public types or focused compile-time tests make the shared-versus-per-worker state boundary explicit enough that later tasks cannot accidentally share mutable search heuristics
- [ ] #7 The existing one-worker behavior remains unchanged
<!-- AC:END -->
