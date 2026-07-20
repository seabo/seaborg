---
id: TASK-64.16.1
title: Specify the Lazy SMP search-team contract
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-19 23:23'
updated_date: '2026-07-20 13:54'
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
- [x] #1 A checked-in module-level contract defines one master, zero or more helpers, team identity, shared state, per-worker state, and the authoritative-result rule
- [x] #2 The contract defines completed, cancelled, failed, and panicked team outcomes and when the single explicit completion signal may be emitted
- [x] #3 The contract specifies fixed-depth, time, node, and infinite limit semantics for the complete team, including which worker decides normal completion
- [x] #4 The contract states that TT age advances once per root search, every worker shares one table allocation, and clearing or replacement occurs only after all workers release it
- [x] #5 The contract preserves legal root fallback and prohibits partial or aborted iterations from becoming official results
- [x] #6 Public types or focused compile-time tests make the shared-versus-per-worker state boundary explicit enough that later tasks cannot accidentally share mutable search heuristics
- [x] #7 The existing one-worker behavior remains unchanged
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reattach existing task branch/worktree (Changes Requested rework after merge-time integration ejection).
2. Merge pinned current master SHA 1a5c1ef into the branch (pinned-SHA workflow) so the branch compiles against the base it will merge onto; expect a clean auto-merge of search.rs plus the KillerTable API change from TASK-64.3.
3. Fix the integration failure: update the two one-arg KillerTable::new(1) calls in the search/team.rs compile-time test to the two-arg (plies, slots) signature introduced by TASK-64.3, mirroring how Search constructs its table (import MAX_KILLER_SLOTS in the tests module).
4. Re-run the required checks (fmt, clippy -D warnings, test --workspace) plus focused search::team tests.
5. Commit, write the review handoff, move to In Review. Prior approval pinned to 20aa3fb is void.
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

--- Rework attempt 3 (merge-integration failure) ---
Merge-time integration ejection (comment #5): the approved target 20aa3fb was textually clean but failed to compile once merged onto master 1a5c1ef, because TASK-64.3 landed on master after this task's original base and changed KillerTable::new from one arg to two (plies, slots). The compile-time test in search/team.rs still called the one-arg form.

Fix (pinned-SHA workflow):
- Merged pinned current master 1a5c1ef into the task branch (merge commit 99221ea) so the branch compiles against the base it will integrate onto. search.rs auto-merged cleanly; the branch now carries TASK-64.3's two-arg KillerTable API.
- Updated the two KillerTable::new(1) call sites in the search/team::tests::shared_state_is_borrowed_and_per_worker_state_is_owned test to KillerTable::new(1, MAX_KILLER_SLOTS), importing MAX_KILLER_SLOTS in the tests module (commit acdd363). Table dimensions are irrelevant to what the test asserts (per-worker state is moved, not borrowed); the values only need to be valid.
No contract prose or production search path changed in this rework.

Verification (worktree, base 1a5c1ef, target acdd363):
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo test --workspace: all passed; engine 287 passed (up from 275: TASK-64.3 added killer tests) incl. the 3 search::team::tests
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

author: @claude
created: 2026-07-20 12:00
---
Review attempt: 2
Reviewed branch: task-64.16.1-lazy-smp-team-contract
Reviewed implementation: 20aa3fbad6c40a2078e8360afd9a89f69d4bb77a
Base: f84b6d8c6afd11c30841cf287a38fa82daacd648
Verdict: approved

REV-1-01 [P1] resolved. The prior finding was that §2/§4 wrongly asserted the completion signal is emitted only after every worker releases its table clone, and claimed this is today's one-worker rule. The rework (only file: engine/src/search/team.rs, plus the task record) corrects this and now matches the code:
- §2 (team.rs:99-115): the signal's sole precondition is that the master's outcome is fixed; an explicit paragraph states the signal does NOT imply table release and that a worker (incl. the emitting master) may still hold its Arc<Table> clone when it fires. Verified against SearchEngine::start (engine/src/search.rs:322,343-357): finished_tx.send(()) runs before the closure returns and drops the `table` clone.
- §4 (team.rs:152-165): clear/replace reachability is attributed to the join-on-drop guarantee, not the withheld signal. Verified against SearchHandle::drop (engine/src/search.rs:440-445) cancel+join, clear_hash's Arc::get_mut (engine/src/search.rs:283-287), and the test dropping_a_search_handle_releases_the_table_for_a_later_clear (engine/src/search.rs:3136-3146).
- Preamble (team.rs:28-32): join-on-drop added as an explicit fifth preserved guarantee, distinct from the completion signal. The false 'reduces to today's rule: the one worker signals after ... it has dropped the table' claim is removed.

Acceptance criteria (all proven):
- AC#1 §1 team.rs:34-80: master/helpers, team identity, shared vs per-worker state, master-authoritative rule.
- AC#2 §2 team.rs:82-115: four outcomes + single completion signal precondition.
- AC#3 §3 team.rs:117-141: depth/time/nodes/infinite; master decides normal completion.
- AC#4 §4 team.rs:143-165: age once per root search; one shared Arc<Table>; clear/replace only after all release.
- AC#5 §5 team.rs:167-181: legal root fallback; no partial/aborted result becomes official.
- AC#6 team.rs:197-278: public SharedTeamState/PerWorkerState marker traits + 3 compile-time tests (all pass).
- AC#7: no production search path changed; only `pub mod team;` added; 275 engine tests pass.

Scope: base..target changes only engine/src/search.rs (+5, the module declaration) and the new engine/src/search/team.rs; commits after target touch only the task markdown. No opaque task-ID/AC/finding citations in the contract prose. No new #[allow]. cargo doc emits only 6 pre-existing private-intra-doc-link warnings (eval/search/tt/score); none from the team module.

Verification (worktree, base f84b6d8, target 20aa3fb):
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (confirmed with a fresh CARGO_TARGET_DIR)
- cargo test --workspace: all passed; engine 275 passed incl. the 3 search::team::tests
---

author: @claude
created: 2026-07-20 13:05
---
Merge attempt: eject (integration failure)
Primary tip tested: 1a5c1ef1d9193d719753b6af29a241731cf06c4a (master)
Approved target: 20aa3fbad6c40a2078e8360afd9a89f69d4bb77a
Trial merge: non-ff merge of branch tip e424aaa onto detached primary tip 1a5c1ef

The merge is textually clean (search.rs auto-merges) but the integrated result does NOT compile, so it is ejected without advancing primary. Master has been left at 1a5c1ef.

Cause: TASK-64.3 (killer table repair) landed on master after this task's base f84b6d8 and changed KillerTable::new from one argument to two — it is now `pub fn new(plies: usize, slots: usize)` (engine/src/killer.rs:50 at 1a5c1ef). This task's compile-time test still calls the old one-arg form:
  engine/src/search/team.rs:275: issue_to_worker(&table, KillerTable::new(1))
  engine/src/search/team.rs:276: issue_to_worker(&table, KillerTable::new(1))

Failing command (on the trial merge commit, fresh CARGO_TARGET_DIR):
  cargo clippy --workspace --all-targets --all-features -- -D warnings
Evidence:
  error[E0061]: this function takes 2 arguments but 1 argument was supplied
    --> engine/src/search/team.rs:275:56  (argument #2 of type `usize` is missing)
  error[E0061]: this function takes 2 arguments but 1 argument was supplied
    --> engine/src/search/team.rs:276:56
  error: could not compile `engine` (lib test) due to 2 previous errors

Rework: integrate current master (re-base the task onto a current master SHA per the pinned-SHA workflow) and update the two KillerTable::new call sites in the compile-time test to the two-argument signature (a plies and slots value, e.g. mirroring how Search constructs its KillerTable at the new base). Then re-run fmt/clippy/test and re-hand off for review; approval pinned to 20aa3fb is void because the integrated result fails.
---

author: @claude
created: 2026-07-20 13:54
---
Implementation handoff
Branch: task-64.16.1-lazy-smp-team-contract
Worktree: /Users/seabo/seaborg-worktrees/task-64.16.1-lazy-smp-team-contract
Base: 1a5c1ef1d9193d719753b6af29a241731cf06c4a (current master; merged into the branch at 99221ea)
Implementation target: acdd363
Resolved findings: none open. Resolves the merge-integration ejection in comment #5 (KillerTable::new arity). Prior approval pinned to 20aa3fb is void because the integrated result failed to compile.
Verification:
- cargo fmt --check: OK
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (no warnings)
- cargo test --workspace: all passed (engine 287 passed incl. 3 search::team::tests)
Known failures: none. cargo doc -p engine --no-deps still emits pre-existing private-intra-doc-link warnings in eval.rs/search.rs/tt.rs (present at base 1a5c1ef, outside this task's scope); the team module adds none. cargo doc is not a repository-required check.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds engine/src/search/team.rs (declared via `pub mod team;` in search.rs): the checked-in module-level Lazy SMP search-team contract. Module `//!` docs specify team composition (one master, zero+ helpers), team identity, shared vs per-worker state, the master-authoritative-result rule (§1); the four team outcomes and the single explicit completion signal, whose sole precondition is the master's outcome being fixed (§2); depth/time/nodes/infinite limit semantics with the master deciding normal completion (§3); TT rules — age advances once per root search, one shared Arc<Table> owned by no worker, clear/replace only after every worker releases the Arc, reachable via the join-on-drop guarantee not the signal (§4); legal root fallback and no-partial/aborted-result rules (§5); and the one-worker degenerate case (§6). AC#6 is enforced by public marker traits SharedTeamState (: Send+Sync) / PerWorkerState with three compile-time tests. No production search path changed (AC#7). Verified at target 20aa3fbad6c40a2078e8360afd9a89f69d4bb77a: cargo fmt --check OK; cargo clippy --workspace --all-targets --all-features -- -D warnings clean (confirmed with a fresh CARGO_TARGET_DIR); cargo test --workspace all passed (engine 275 passed incl. the 3 search::team::tests). REV-1-01 resolved: §2/§4 and the preamble now correctly describe the signal firing while a worker still holds its table clone, with clear-safety attributed to join-on-drop, matching SearchEngine::start/SearchHandle::drop/clear_hash.
<!-- SECTION:FINAL_SUMMARY:END -->
