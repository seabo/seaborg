---
id: TASK-64.2
title: 'Activate the history heuristic with bonus, malus and aging'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 13:30'
updated_date: '2026-07-19 21:20'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace the unsigned accumulating history entry with a signed, bounded gravity update and depth-squared bonus/malus helpers, retaining per-search lifetime so iterative-deepening evidence carries forward without leaking between unrelated UCI searches.
2. Track previously searched quiet moves at each main-search node; on a quiet beta cutoff reward the cutoff move and penalize those failed quiet predecessors.
3. Convert history values to compact ordering scores with explicit saturation, preserving ordering beyond the i16 boundary without a wrapping cast or increasing the per-ply move-ordering footprint.
4. Add focused history and staged-ordering regressions covering bounded updates, bonus/malus behavior, trained quiet ordering, and values beyond the i16 boundary.
5. Run focused tests, the TASK-27 strength-regression smoke comparison, and all repository-required formatting, strict Clippy, and workspace tests; record evidence and hand off an immutable commit for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented signed butterfly history with depth-squared evidence and bounded gravity updates in [-32,768, 32,768]. A quiet beta cutoff receives the positive update; every fully searched quiet predecessor at that node receives the matching malus. History-to-ordering conversion saturates explicitly to i16, so a table value of 32,768 remains ahead of an untrained move instead of wrapping negative, while OrderedMoves retains its existing compact footprint.

Persistence decision: retain the existing per-search lifetime. Evidence is shared across iterative-deepening iterations within one Search::run, then reset when that run finishes; it does not persist across moves within a game. Search objects and their positions are request-specific today, so carrying this table across moves would require a new game-owned heuristic boundary and reset semantics. Keeping it local avoids leaking stale evidence across unrelated searches while still adapting throughout the tree where the gathered evidence is relevant.

TASK-27 strength smoke: baseline c7826f15b267cd89b0c1c02c97b5294f6ec9bf57 versus candidate working tree, optimized cargo build --release --bin seaborg, FastChess alpha 1.5.0, 4 paired-colour games at depth=4, concurrency=2, Hash=64, Threads=1. Result: non-authoritative INCONCLUSIVE, 2 wins / 0 draws / 2 losses, LLR 0.0 within [-2.94, 2.94], 0 forfeits, 0 crashes, runner exit 0. This smoke run establishes successful match integration but is too small to claim a strength result.
<!-- SECTION:NOTES:END -->
