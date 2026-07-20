---
id: TASK-64.16.1
title: Specify the Lazy SMP search-team contract
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-19 23:23'
updated_date: '2026-07-20 10:16'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added engine/src/search/team.rs (declared `pub mod team;` in search.rs) as the checked-in module-level Lazy SMP search-team contract.

Contract prose (module `//!` docs), mapped to acceptance criteria:
- AC#1 §1: team = one master + zero-or-more helpers; team identity = one `go` sharing one table/stop-flag/limit; shared team state (Table, cancellation flag, limit) vs per-worker state (position copy, eval + eval stack, killer/history/PV tables, tracer, per-ply stack, root fallback); authoritative-result rule = master's last completed iteration, helpers influence only via the shared TT, cross-worker voting deferred to a later task.
- AC#2 §2: four outcomes (Completed/Cancelled/Failed/Panicked) with helper-vs-master degradation rules; single explicit completion signal emitted once, only after the master's outcome is fixed AND every worker has released the shared table (generalizes TASK-35; underpins the clear-after-completion guarantee).
- AC#3 §3: Depth/Time/Nodes/Infinite semantics; the master decides normal completion for every limit; node budget stays reproducible by binding authoritative completion to the master's own counter; Infinite has no normal-completion path.
- AC#4 §4: age advances once per root search (per team, not per worker); one Arc<Table> shared, owned by no worker; clear/replace only after all workers release the Arc (Arc::get_mut boundary from TASK-57), reachable because the completion signal is withheld until release.
- AC#5 §5: master establishes a legal root fallback before any node and honours cancellation only after (TASK-45); no partial/aborted iteration becomes official, last completed PV preserved (TASK-46); helper partial work influences the move only via committed TT entries.
- AC#6: public marker traits SharedTeamState (: Send + Sync, impl for Table) and PerWorkerState (impl for KillerTable/HistoryTable/PVTable/Tracer/EvalState), plus three compile-time tests: shared allocation is Send+Sync; each heuristic is classified per-worker; the issue-to-worker seam borrows shared state and moves per-worker state so a heuristic cannot be shared by accident.
- AC#7 §6: no production search path changed; the zero-helper team is exactly today's engine.

Note on doc links: kept the contract docs solely as the module's inner `//!` and used a plain (non-doc) comment on the `pub mod team;` line. An outer `///` there concatenates with the inner docs and resolves intra-doc links in the parent (search) scope, which broke every `super::`/same-module link; `cargo doc -p engine --no-deps` now emits no team-module warnings (the 6 remaining private-intra-doc-link warnings in eval/search/tt pre-exist on master).

Verification (in worktree, base f84b6d8):
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo test --workspace: all passed (engine unit tests 275 passed incl. 3 new team tests; other suites green)
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 10:16
---
Implementation handoff
Branch: task-64.16.1-lazy-smp-team-contract
Worktree: /Users/seabo/seaborg-worktrees/task-64.16.1-lazy-smp-team-contract
Base: f84b6d8c6afd11c30841cf287a38fa82daacd648
Implementation target: 6cf81946bdad68093067d6cde212b7d4684e319f
Resolved findings: none
Verification:
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (no warnings)
- cargo test --workspace: all passed (3 new tests in search::team::tests)
Known failures: none. cargo doc -p engine --no-deps emits 6 private-intra-doc-link warnings in eval.rs/search.rs/tt.rs that reproduce at base f84b6d8 and are outside this task's scope; the team module adds no doc warnings. cargo doc is not one of the repository-required checks.
---
<!-- COMMENTS:END -->
