---
id: TASK-45
title: Honor UCI cancellation after recording a legal root fallback
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-18 18:28'
updated_date: '2026-07-18 23:39'
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
- [ ] #1 An immediate stop during the first iteration returns a legal bestmove whenever the root position has a legal move, including when cancellation wins the race before any searched root move completes
- [ ] #2 Explicit cancellation can terminate depth 1 without waiting for the full quiescence tree, and deterministic tests prove the cancellation path rather than relying only on a loose wall-clock assertion
- [ ] #3 Quit, stdin EOF, replacement go, and other active-search replacement paths preserve their current legal-bestmove and teardown behavior because they share the cancellation mechanism
- [ ] #4 Terminal root positions still return bestmove 0000
- [ ] #5 Time-deadline behavior remains unchanged: zero and near-zero budgets still return a legal move; TASK-29 may cap quiet check extensions on its own merits but is not responsible for bounding capture/promotion interleaving or the total depth-1 quiescence tree
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
