---
id: TASK-64.16.1
title: Specify the Lazy SMP search-team contract
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-19 23:23'
updated_date: '2026-07-20 11:40'
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
1. Reuse existing branch/worktree; address blocking REV-1-01 only, preserving the approved contract prose elsewhere.
2. REV-1-01: the contract's §2 condition 2 and §4 wrongly assert the single completion signal is emitted only after every worker releases its table clone, and claim this is today's one-worker rule. It is not: in SearchEngine::start the worker sends finished_tx while still owning its Arc<Table> clone (released only on closure return); clear-safety is guaranteed by join-on-drop (SearchHandle::drop cancels+joins), not by the signal.
3. Rewrite §2 so the signal's sole precondition is that the master's outcome is fixed, and state explicitly that a worker (incl. the master) may still hold its table clone when the signal fires, exactly as the one worker does today.
4. Rewrite §4 so clear/replacement reachability is attributed to joining every worker (the join-on-drop guarantee via Arc::get_mut), not to the withheld signal; generalize join-on-drop to the whole team.
5. Make the preserved join-on-drop guarantee explicit in the preamble.
6. Run cargo fmt --check, clippy -D warnings, cargo test --workspace; record Resolved REV-1-01 with evidence in the handoff.
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

--- Rework attempt 2 ---
Resolved REV-1-01: the contract's §2 and §4 misdescribed the completion-signal vs table-release ordering.
Behaviour changed (documentation only; no production search path touched):
- §2: the single completion signal's only precondition is now that the master's outcome is fixed. Added an explicit paragraph stating the signal does NOT imply table release — a worker, including the master that emits it, may still hold its Arc<Table> clone when the signal fires, exactly as SearchEngine::start does today (finished_tx.send runs before the closure returns and drops the `table` clone). Clear-safety is attributed to a separate guarantee (join), not the signal.
- §4: clear/replacement reachability is now attributed to the join-on-drop guarantee (dropping/waiting the handle cancels+joins every worker, so Arc::get_mut in clear_hash succeeds because the join released the last clone), explicitly NOT to the withheld signal. Added the correct owner protocol: join the team, then clear — never clear on the signal alone.
- Preamble: added join-on-drop as an explicit fifth preserved guarantee, distinguished from the completion signal.
Removed the false claim that "with zero helpers this reduces to today's rule: the one worker signals after it has dropped the table."

Verification (worktree, base f84b6d8, target 20aa3fb):
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo test --workspace: all passed (engine 275 passed incl. the 3 search::team::tests and dropping_a_search_handle_releases_the_table_for_a_later_clear)
- cargo doc -p engine --no-deps: same 6 pre-existing private-intra-doc-link warnings in eval.rs/search.rs/tt.rs; none from the team module; the new SearchHandle/SearchEngine/Table links resolve cleanly (not a required check)
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

author: @claude
created: 2026-07-20 11:32
---
Review attempt: 1
Reviewed branch: task-64.16.1-lazy-smp-team-contract
Reviewed implementation: 6cf81946bdad68093067d6cde212b7d4684e319f
Verdict: changes_requested

REV-1-01 [P1] Contract misstates the completion-signal vs. table-release ordering
Location: engine/src/search/team.rs:106-107 (also the §2 signal conditions at team.rs:100-102 and the §4 clear rule at team.rs:148-150)
Impact: The contract is the authoritative source that is supposed to stop later Lazy SMP tasks from relying on implicit ownership assumptions, but §2 asserts the single completion signal may be emitted only after "every worker in the team ... has released its clone of the shared table", and states: "With zero helpers this reduces to today's rule: the one worker signals after its outcome is determined and it has dropped the table." Today's one worker does the opposite. In SearchEngine::start (engine/src/search.rs:340-357) it fixes the outcome, drops only the Search (which just ends the &Table borrow), then calls finished_tx.send(()) while it still owns the `table` Arc<Table> clone; that clone is released only when the closure returns. The real table-release-for-clear guarantee is join-on-drop, not the signal: SearchHandle::drop (engine/src/search.rs:424-445) cancels and joins the worker precisely so a detached worker cannot outlive its handle still holding a table clone, and dropping_a_search_handle_releases_the_table_for_a_later_clear (engine/src/search.rs:3131-3146) tests exactly that. §4 (team.rs:148-150) compounds the error by attributing clear-reachability to the withheld signal ("Because the completion signal (§2) is withheld until every worker has released the table, an owner that waited for completion can always clear") instead of to the join. A later task that trusts the contract -- clearing on the completion signal without joining, as "the session owner clear[s] or resize[s] the table immediately after it observes completion" invites -- reintroduces the intermittent ucinewgame panic at Arc::get_mut(&mut self.table).expect(...) in clear_hash (engine/src/search.rs:283-287) that join-on-drop was written to prevent. This leaves AC#2 (when the single completion signal may be emitted) and AC#7 (existing one-worker behavior accurately preserved) unproven.
Reproduction: Read engine/src/search.rs:340-357 -- finished_tx.send(()) executes before the closure return that drops the `table` Arc clone, so the allocation still has two owners at signal time. Read SearchHandle::drop (engine/src/search.rs:424-445) and the test at engine/src/search.rs:3131-3146 -- release for a later clear is guaranteed by joining the worker on handle drop/wait. Compare against the contract at team.rs:100-107 and team.rs:148-150, which claim the signal itself implies release and that this is today's behavior.
Expected: Describe the current one-worker completion path accurately: the worker emits the single completion signal while it still holds its shared-table clone, and table release (hence clear-safety) is guaranteed by joining the worker on handle drop/wait, not by the signal. If the team design deliberately intends to strengthen this so the signal implies release, state it as an explicit change from today's behavior and reconcile it with the preserved join-on-drop guarantee (specify who releases and who joins helpers before the signal is emitted), rather than asserting the signal-after-release ordering is already today's rule.

Verification:
- cargo fmt --check: OK (target 6cf8194)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo test --workspace: all passed; engine 275 passed incl. the 3 new search::team::tests and dropping_a_search_handle_releases_the_table_for_a_later_clear
- cargo doc -p engine --no-deps: 6 pre-existing private-intra-doc-link warnings in eval.rs/search.rs/tt.rs, none from the team module (not a required check)
---

author: @claude
created: 2026-07-20 11:40
---
Implementation handoff
Branch: task-64.16.1-lazy-smp-team-contract
Worktree: /Users/seabo/seaborg-worktrees/task-64.16.1-lazy-smp-team-contract
Base: f84b6d8c6afd11c30841cf287a38fa82daacd648
Implementation target: 20aa3fbad6c40a2078e8360afd9a89f69d4bb77a
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (no warnings)
- cargo test --workspace: all passed (engine 275 passed incl. 3 search::team::tests and dropping_a_search_handle_releases_the_table_for_a_later_clear)
Known failures: none. cargo doc -p engine --no-deps emits 6 pre-existing private-intra-doc-link warnings in eval.rs/search.rs/tt.rs that reproduce at base f84b6d8 and are outside this task's scope; the team module adds no doc warnings. cargo doc is not a repository-required check.
---
<!-- COMMENTS:END -->
