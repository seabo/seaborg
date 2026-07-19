---
id: TASK-45
title: Honor UCI cancellation after recording a legal root fallback
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-18 18:28'
updated_date: '2026-07-19 00:06'
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
1. Split the single `min_search_complete` abort gate into two independent gates in `Search`:
   - `root_fallback_ready` / `root_fallback: Option<Move>` gates the explicit cancellation flag.
   - `min_search_complete` continues to gate the time deadline only (TASK-32 policy unchanged).
2. Establish the root fallback in `iterative_deepening` before the first iteration: generate legal
   root moves and record the first one (`None` for a terminal root), then set `root_fallback_ready`.
   Generation is finite and runs before any node is searched, so a legal bestmove exists before
   cancellation can ever be honored.
3. Upgrade the fallback at the root move loop: when a root move's search returns while not stopping
   and improves `best_value`, record it as the fallback, so a cancellation mid-first-ply returns the
   best fully searched root move rather than an arbitrary generated one.
4. Rewrite `stopping()`: cancellation flag aborts as soon as `root_fallback_ready`; the time
   deadline is still suppressed until `min_search_complete`.
5. `iterative_deepening` returns the fallback `SearchResult` (depth 0, zero score) when no
   iteration completed, so an early-cancelled search still yields a legal bestmove; terminal roots
   still yield `None` -> `bestmove 0000`.
6. Tests: deterministic node-count proof that a pre-set cancellation flag aborts depth 1 without
   searching the quiescence tree; fallback legality with cancellation winning the race; fallback
   tracks the best completed root move; terminal root still returns no move; update the two existing
   tests that emulated the armed state via `min_search_complete`; keep time-deadline tests intact.
7. Run cargo fmt --check, strict clippy, and cargo test --workspace.
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
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Established a legal root fallback before honoring explicit cancellation, allowing depth-1 search to abort immediately while preserving legal bestmove, terminal 0000, driver teardown, and time-deadline behavior. Verified at implementation c303c08 with deterministic cancellation tests (zero visited nodes), UCI EOF/replacement regressions, formatting, fresh-target strict Clippy, full workspace tests, and base/target performance benchmarks.
<!-- SECTION:FINAL_SUMMARY:END -->
