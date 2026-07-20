---
id: TASK-64.16.1
title: Specify the Lazy SMP search-team contract
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 23:23'
updated_date: '2026-07-20 10:09'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a new `search::team` submodule (engine/src/search/team.rs, declared `pub mod team;` in search.rs) that is the checked-in home for the Lazy SMP search-team contract. It adds documentation and classification types only; it changes no existing search code path, so one-worker behavior is unchanged (AC#7).
2. Write the module-level contract as module doc covering: team composition (one master + zero-or-more helpers), team identity, shared team state vs per-worker state, and the authoritative-result rule (master's last completed iteration; helpers influence only via the shared TT; no cross-worker voting in baseline) (AC#1).
3. Document the four team outcomes (Completed/Cancelled/Failed/Panicked) and the single explicit completion signal: emitted exactly once per team, after the master's outcome is fixed and every worker has released the shared table (generalizes TASK-35's finished channel; preserves join-on-drop) (AC#2).
4. Document limit semantics for Depth/Time/Nodes/Infinite for the whole team, stating the master decides normal completion and helpers never turn a limit into team completion; node budget stays reproducible by binding authoritative completion to the master's own counter (AC#3).
5. Document TT rules: age advances once per root search (per team, not per worker); one Arc<Table> allocation shared by all workers; clear/replacement only after all workers release it (Arc::get_mut boundary from TASK-57) (AC#4).
6. Document the fallback + no-partial-results rules: master establishes a legal root fallback before any node (TASK-45); a partial or aborted iteration never becomes the official result (TASK-46); helpers' partial work influences only the TT (AC#5).
7. Add public classification marker traits SharedTeamState (: Send + Sync) and PerWorkerState, implement them for the shared Table and the per-worker heuristics (KillerTable, HistoryTable, PVTable, Tracer, EvalState), and add focused compile-time tests: assert the shared allocation is Send+Sync, assert each heuristic is classified per-worker, and prove the worker-issue seam borrows shared state while moving per-worker state so heuristics cannot be accidentally shared (AC#6).
8. Run cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace; record results in the handoff.
<!-- SECTION:PLAN:END -->
