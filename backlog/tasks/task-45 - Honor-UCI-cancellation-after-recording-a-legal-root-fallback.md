---
id: TASK-45
title: Honor UCI cancellation after recording a legal root fallback
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 18:28'
updated_date: '2026-07-19 00:31'
labels:
  - engine
  - search
  - uci
dependencies:
  - TASK-37
references:
  - engine/src/search.rs
  - engine/src/engine.rs
documentation:
  - doc-3
priority: medium
type: bug
ordinal: 46000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
UCI stop, quit, EOF, and commands that replace an active search all call SearchHandle::cancel() and synchronously wait for the worker. TASK-32 deliberately ignores that cancellation until depth 1 plus quiescence completes, guaranteeing a legal bestmove but leaving prompt explicit cancellation dependent on an unbounded quiescence tree.

TASK-39 measured the window as very small on its adversarial corpus (16,000 warmed immediate-stop samples; every median at or below 1.162 ms and an overall maximum of 5.820 ms), but established that the code offers no practically small structural worst-case bound. A TASK-29 cap on consecutive quiet check extensions does not provide that bound: the adversarial search found quiet-check chains no longer than 5 while capture/promotion interleaving produced reachable trees over 20 million nodes and 55 plies. TASK-29 may cap check extensions on its own merits, but it does not bound the total depth-1 quiescence tree. UCI gives no numeric stop deadline, and tournament runners can apply zero or configured time margin, so prompt explicit cancellation should not rely on typical-position timing.

Change cancellation semantics without weakening the TASK-32/EOF invariant: establish a legal root fallback before cancellation can be honored, then allow the explicit cancellation flag to stop depth 1 immediately. Keep the time-deadline policy unchanged and separate from explicit cancellation; this ticket does not assign TASK-29 responsibility for bounding total depth-1 quiescence work. Coordinate with TASK-37's driver-level EOF regression coverage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An immediate stop during the first iteration returns a legal bestmove whenever the root position has a legal move, including when cancellation wins the race before any searched root move completes
- [x] #2 Explicit cancellation can terminate depth 1 without waiting for the full quiescence tree, and deterministic tests prove the cancellation path rather than relying only on a loose wall-clock assertion
- [x] #3 Quit, stdin EOF, replacement go, and other active-search replacement paths preserve their current legal-bestmove and teardown behavior because they share the cancellation mechanism
- [x] #4 Terminal root positions still return bestmove 0000
- [x] #5 Time-deadline behavior remains unchanged: zero and near-zero budgets still return a legal move; TASK-29 may cap quiet check extensions on its own merits but is not responsible for bounding capture/promotion interleaving or the total depth-1 quiescence tree
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Rebase the rework context by merging the current primary tip into the persistent TASK-45 branch, preserving immutable prior history.
2. Resolve all three TASK-46 overlaps in engine/src/search.rs: retain abort_after_nodes fields/initialization and combine its deterministic node-stop hook with TASK-45 explicit-cancellation gating on root_fallback_ready and deadline gating on min_search_complete.
3. Inspect the integrated search and cancellation tests; add or adjust regression coverage if the combined semantics are not already pinned.
4. Record resolution of the merge-gate finding, run focused cancellation/time-limit tests, then cargo fmt --check, strict workspace Clippy, and cargo test --workspace.
5. Commit the integrated implementation, record a new immutable target and handoff, and return TASK-45 to In Review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Split the single abort gate in `Search` into two.

- `Search::establish_root_fallback` runs once at the top of `iterative_deepening`, before any
  node is searched. It records the first generated legal root move (`None` for a terminal root)
  and sets `root_fallback_ready`.
- `stopping()` now returns `root_fallback_ready` as soon as the cancellation flag is set, so an
  explicit stop no longer waits on the depth-1 quiescence tree. The time deadline is unchanged: it
  is still suppressed until `min_search_complete` (the completed first ply), so zero and near-zero
  budgets still return a searched move rather than the unsearched fallback.
- The root move loop upgrades `root_fallback` to each root move that improves `best_value` while
  not stopping, so a cancellation mid-first-ply reports a fully searched move rather than the
  arbitrary first generated one. Moves whose subtree was aborted carry a meaningless score and are
  not adopted.
- `iterative_deepening` returns the fallback as a `SearchResult` (depth 0, zero score) when no
  iteration completed. Only `best_move` is consumed by the UCI bestmove line and by `game.rs`.
- Aborted nodes still return before their transposition-table write, so honoring cancellation
  earlier writes no new polluted entries.

Tests: `cancellation_stops_the_first_iteration_without_searching_it` is the deterministic proof —
against a quiescence-heavy baseline of >1000 nodes, a pre-set cancellation flag returns a legal
fallback having visited exactly 0 nodes. Added `immediate_cancellation_returns_a_legal_move`,
`the_root_fallback_tracks_the_best_searched_root_move`, `cancelled_terminal_root_reports_no_move`,
and `cancellation_is_suppressed_only_until_the_root_fallback_exists`. Renamed and rewrote
`aborts_are_suppressed_only_until_the_first_ply_completes` as
`the_time_deadline_is_suppressed_until_the_first_ply_completes`, since the two signals are no
longer gated together. `quiescence_abort_with_legal_evasions_is_not_checkmate` now arms
`root_fallback_ready` instead of `min_search_complete`.

Rework after merge attempt 1: merged primary 4d48c359, which includes TASK-46's aborted-subtree propagation. Resolved all three conflicts by retaining root_fallback_ready/root_fallback alongside the test-only abort_after_nodes hook. Search::stopping now gives the deterministic test hook unconditional priority, honors explicit cancellation once root_fallback_ready is set, and continues to gate only the wall-clock deadline on min_search_complete. Existing TASK-45, TASK-46, TASK-37, and replacement-search focused regressions all pass; no additional test was needed because the combined semantics are directly covered.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-18 23:45
---
Implementation handoff
Branch: task-45-honor-cancellation-after-root-fallback
Worktree: /Users/seabo/seaborg-worktrees/task-45-honor-cancellation-after-root-fallback
Base: 092eb88d6c9635fff5043a918525390dd2c93e13
Implementation target: c303c08
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass (171 + 35 + 5 + 1 tests, 0 failed, 2 ignored)
Known failures: none
---

author: @codex
created: 2026-07-19 00:06
---
Review attempt: 1
Reviewed branch: task-45-honor-cancellation-after-root-fallback
Reviewed implementation: c303c08
Verdict: approved

No blocking findings. The base-to-target diff is scoped, the implementation target descends from recorded base 092eb88d6c9635fff5043a918525390dd2c93e13, and the sole later commit contains handoff metadata only.

Verification:
- cargo fmt --check: pass
- CARGO_TARGET_DIR=<fresh temp> cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace --quiet: pass (35 core; 171 engine passed, 2 ignored; 5 metadata; 1 doc)
- cargo test -p engine cancellation -- --nocapture: pass (3)
- cargo test -p engine time_limited_search -- --nocapture: pass (1)
- cargo test -p engine stdin_eof -- --nocapture: pass (2)
- cargo test -p engine replacement_stop_and_quit_are_serialized -- --nocapture: pass (1)
- cargo bench --bench perft --bench movegen, base vs target: movegen 194.76 ns vs 189.91 ns; perft 22.708 ms vs 22.570 ms; no regression
---

author: @claude
created: 2026-07-19 00:27
---
Merge attempt: 1
Primary tip tested: 4d48c35917a2955550f5a0bbc6a0120d3b0cc957
Merged: decb102 (approved target c303c08)
Result: ejected — textual conflict, no integrated checks run

Failing command: `git merge --no-ff decb102`
Evidence:
```
Auto-merging engine/src/search.rs
CONFLICT (content): Merge conflict in engine/src/search.rs
```

Three conflicting regions, all against TASK-46 (merge d279898, approved target 35b4994), which changed the same abort machinery in `engine/src/search.rs`:

1. `Search` struct fields (~line 419): TASK-46 added `#[cfg(test)] abort_after_nodes: Option<usize>`; TASK-45 added `root_fallback_ready` / `root_fallback` at the same insertion point.
2. `Search` constructor (~line 478): the matching initializers collide at the same position.
3. `stopping()` (~line 899): a genuine semantic overlap, not mere adjacency. TASK-46 rewrote the body to short-circuit on `abort_after_nodes` and return `stopping || deadline`; TASK-45 rewrote the same body to return `root_fallback_ready` when the cancellation flag is set and to gate only the time deadline on `min_search_complete`. Rework must combine both: keep the TASK-46 test hook and node-limit short-circuit, and keep TASK-45's split gating (cancellation gated on `root_fallback_ready`, deadline gated on `min_search_complete`).

The primary branch was not advanced; the trial merge was aborted and primary remains at 4d48c35. Merge current primary into the task branch, resolve the above, re-run the required checks, and return the task to In Review — the approval pinned to c303c08 is void once the implementation changes.
---

author: @codex
created: 2026-07-19 00:31
---
Implementation handoff
Branch: task-45-honor-cancellation-after-root-fallback
Worktree: /Users/seabo/seaborg-worktrees/task-45-honor-cancellation-after-root-fallback
Base: 4d48c35917a2955550f5a0bbc6a0120d3b0cc957
Implementation target: 7532ef9b0c1def755484e3456aba5073618d41b0
Resolved findings: merge attempt 1 conflict recorded in comment #3 (no REV ID)
Verification:
- cargo test -p engine cancellation -- --nocapture: pass (3)
- cargo test -p engine time_limited_search -- --nocapture: pass (1)
- cargo test -p engine search::tests::mid_subtree_abort_keeps_the_last_completed_iteration -- --nocapture: pass (1)
- cargo test -p engine search::tests::aborted_child_cannot_score_or_write_its_parent -- --nocapture: pass (1)
- cargo test -p engine engine::tests::stdin_eof_ -- --nocapture: pass (2)
- cargo test -p engine replacement_stop_and_quit_are_serialized -- --nocapture: pass (1)
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass (35 core + 173 engine passed, 2 ignored + 5 integration + 1 doctest; 0 failed)
Known failures: none
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Established a legal root fallback before honoring explicit cancellation, allowing depth-1 search to abort immediately while preserving legal bestmove, terminal 0000, driver teardown, and time-deadline behavior. Verified at implementation c303c08 with deterministic cancellation tests (zero visited nodes), UCI EOF/replacement regressions, formatting, fresh-target strict Clippy, full workspace tests, and base/target performance benchmarks.
<!-- SECTION:FINAL_SUMMARY:END -->
