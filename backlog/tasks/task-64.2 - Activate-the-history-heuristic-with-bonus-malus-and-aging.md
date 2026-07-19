---
id: TASK-64.2
title: 'Activate the history heuristic with bonus, malus and aging'
status: To Do
assignee: []
created_date: '2026-07-19 13:30'
updated_date: '2026-07-19 13:44'
labels:
  - search
  - move-ordering
dependencies: []
references:
  - engine/src/history.rs
  - engine/src/search.rs
  - engine/src/ordering.rs
parent_task_id: TASK-64
priority: high
type: bug
ordinal: 65000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The butterfly history table is allocated, reset per search, and read for quiet move ordering, but it is never written. Quiet moves are consequently ordered by an all-zero table, which is to say not ordered at all.

The only write site is commented out at search.rs:898-903, in the beta-cutoff branch where the killer move is stored. Both read sites are live: `MoveLoader::score_quiets` (search.rs:1488-1499) and `QMoveLoader::score_quiets` (search.rs:1548-1559) each call `history.get_unchecked` and assign the result as the move score. `history.reset()` is called at search.rs:551.

The effect is that the Quiet phase of the staged ordering yields moves in raw generation order. Since the Quiet phase sits between Killers and BadCaptures and covers the large majority of moves at most nodes, this is a substantial ordering loss on its own. It also blocks other work: reduction amounts for late move reductions, and the thresholds for move-count pruning, are conventionally driven by history scores, so those features cannot be tuned meaningfully while the table reads zero.

The table as it stands is a bare u32 butterfly table (history.rs:79-82) whose only mutation is `inc` (history.rs:98), an unguarded AddAssign. Reactivating the commented-out call alone would give a table that only ever grows, has no overflow guard, and never forgets. The work is therefore to make the heuristic correct rather than merely present: a depth-scaled bonus on the cutoff move, a malus applied to the quiet moves that were tried and failed before it, and a gravity or aging scheme that keeps values bounded and lets the table adapt within a search.

Whether history should be retained across moves within a game, rather than reset per search as it is today at search.rs:551, is an open question worth settling here and recording.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A quiet move causing a beta cutoff receives a depth-scaled history bonus
- [ ] #2 Quiet moves searched and failing before the cutoff move receive a malus
- [ ] #3 History values are bounded by a documented gravity or scaling scheme and cannot overflow their storage type
- [ ] #4 Quiet moves in the ordering Quiet phase are demonstrably ordered by history score, verified by a test asserting a known good quiet is yielded before a known poor one after training the table
- [ ] #5 The decision on whether history persists across moves within a game is recorded with rationale
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
- [ ] #7 The history value read at the ordering sites is not narrowed by a truncating cast: search.rs:1499 and search.rs:1559 currently cast a u32 table value to i16, which wraps above 32767 and orders a repeatedly successful quiet move last
- [ ] #8 A test drives a history value past the storage boundary of the ordering score type and asserts the move is still ordered ahead of an untrained move
<!-- AC:END -->
