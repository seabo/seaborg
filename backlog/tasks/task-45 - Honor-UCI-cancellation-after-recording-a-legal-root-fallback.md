---
id: TASK-45
title: Honor UCI cancellation after recording a legal root fallback
status: To Do
assignee: []
created_date: '2026-07-18 18:28'
updated_date: '2026-07-18 21:22'
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
