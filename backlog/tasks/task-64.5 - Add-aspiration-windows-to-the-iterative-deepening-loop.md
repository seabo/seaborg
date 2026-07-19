---
id: TASK-64.5
title: Add aspiration windows to the iterative deepening loop
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 13:31'
updated_date: '2026-07-19 22:50'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add aspiration windows to iterative_deepening. Introduce ASPIRATION_MIN_DEPTH (below which, and for iteration 1, the full window is used, preserving the guaranteed first-ply contract) and an initial centipawn half-width delta.
2. For iteration d >= min depth with a previous score, centre a window on the previous score. Re-search on fail-low (value<=alpha) or fail-high (value>=beta), widening geometrically. A bound whose delta exceeds a cap, or a fail that returns a mate score, snaps to the matching infinity so the loop terminates in a bounded number of re-searches and every returned score comes from a search whose window contained it.
3. Mate/cp-band handling: a helper offsets a centipawn score by a delta and clamps into band; a mate (non-cp) score cannot be nudged by centipawns, so it opens the bound to infinity. Windows derived from a mate previous score fall back to the full window. Guarantees the returned score stays a node score (is_node_score).
4. Abort handling: propagate None from any re-search so the iteration is discarded and its PV table restored (TASK-46 guarantee). Aspiration only runs after min_search_complete, so the first ply is never turned into an unbounded re-search sequence.
5. Tests: unit test the window helper (cp widen/clamp, mate->infinity); regression test a forced-mate-at-root position searched to a depth that engages aspiration, asserting the correct mate node score; test that low-depth iterations still use the full window.
6. Measure node counts at fixed depth on a representative position set vs the base commit (AC#6) and run the TASK-27 strength script (AC#7); record both in implementation notes.
<!-- SECTION:PLAN:END -->
