---
id: TASK-64.6
title: Add a node-count search limit
status: To Do
assignee: []
created_date: '2026-07-19 13:31'
labels:
  - search
  - uci
  - nnue
  - tooling
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/time.rs
  - engine/src/uci.rs
parent_task_id: TASK-64
priority: high
type: feature
ordinal: 69000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The search cannot be bounded by node count. `SearchLimit` (search.rs:36-44) offers only Depth, Time and Infinite, and `TimingMode` (time.rs:24-29) mirrors that with Timed, MoveTime, Depth and Infinite.

This blocks NNUE data generation. Self-play datagen is conventionally run at a fixed node budget rather than a fixed time, because a node budget is reproducible across machines, unaffected by load from concurrent games, and identical between debug and release builds. A time budget gives none of those properties, and a depth budget gives wildly varying effort per position, which biases the resulting label distribution toward positions that happen to be cheap to search.

It is also the natural budget for A/B testing search changes, since it removes machine speed from the comparison, and it is the standard interpretation of the UCI `go nodes` parameter, which the engine does not currently support.

The node counter already exists: `Tracer::all_nodes_visited` is read by `Search::stopping` for deadline throttling (search.rs:1012). The work is to add the limit variant, honour it in the stopping check, and thread it through the UCI go-command parsing.

Two existing guarantees interact with this and must be preserved. The guaranteed first ply (`min_search_complete`, search.rs:460-461, :998-1000) ensures a legal searched move is returned under a zero budget, and the same reasoning applies to a node budget too small to complete a ply. Explicit cancellation is gated separately on `root_fallback_ready` and must remain unthrottled and unaffected.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A node-count search limit exists and terminates the search once the budget is consumed
- [ ] #2 UCI go nodes is parsed and honoured
- [ ] #3 A search under a node limit returns the same best move and score for the same position and limit across repeated runs on the same build
- [ ] #4 A node budget smaller than one full ply still returns a legal searched move, consistent with the existing zero-time-budget guarantee
- [ ] #5 Explicit cancellation remains responsive and unthrottled under a node limit
- [ ] #6 Tests cover budget exhaustion mid-iteration, a budget below one ply, and reproducibility across runs
<!-- AC:END -->
