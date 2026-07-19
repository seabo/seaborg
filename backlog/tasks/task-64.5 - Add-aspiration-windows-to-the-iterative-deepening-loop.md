---
id: TASK-64.5
title: Add aspiration windows to the iterative deepening loop
status: To Do
assignee: []
created_date: '2026-07-19 13:31'
labels:
  - search
  - strength
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: feature
ordinal: 68000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Every iterative-deepening iteration searches the root with a full window. Narrowing that window around the previous iteration's score is one of the cheapest large reductions in node count available to this search.

The loop at search.rs:561-586 calls `self.search::<T, Root>(Score::INF_N, Score::INF_P, d)` for every d. Nothing carries the previous iteration's score forward, so each iteration re-derives the root value from an unbounded window and forfeits the cutoffs a narrow window would produce throughout the tree.

The technique is to search iteration d with a window centred on the score returned by iteration d-1, widening and re-searching on a fail high or fail low. The design questions to settle are the initial window width, the widening schedule, the depth below which aspiration is not worth applying, and what happens when a re-search is interrupted.

Two existing invariants constrain the implementation and must be preserved. First, `iterative_deepening` only commits a result when `self.search` returns Some, and an aborted iteration is discarded along with its PV table (search.rs:566-571); a fail-low or fail-high re-search must not weaken that, and TASK-46 established that aborted subtrees cannot contribute scores. Second, `min_search_complete` (search.rs:585) guarantees the first full ply completes against the clock regardless of budget, so aspiration must not turn iteration 1 into an unbounded sequence of re-searches.

Mate scores are position-relative in this engine and clamped to the mate band by mate-distance pruning (search.rs:690-691). A window derived from a mate score therefore needs care to stay inside the encoding that `Score::is_node_score` enforces; TASK-56 and the out-of-band window tests in search.rs are the relevant precedent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Iteration d is searched with a window derived from the score of iteration d-1, above a documented minimum depth
- [ ] #2 A fail high or fail low triggers a widening re-search under a documented schedule, and the reported score is always from a search whose window contained it
- [ ] #3 An aborted re-search discards the iteration rather than committing a bound as a result, preserving the TASK-46 guarantee
- [ ] #4 The guaranteed first-ply completion is preserved and cannot be extended indefinitely by re-searches
- [ ] #5 Windows derived from mate scores remain inside the node score band, with a test covering a position with a forced mate at the root
- [ ] #6 Node counts at fixed depth on a representative position set are reduced relative to the full-window baseline, with figures recorded in the implementation notes
- [ ] #7 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
