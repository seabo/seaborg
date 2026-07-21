---
id: TASK-64.18
title: Remove dead quiescence ordering paths and guard the ordering buffer capacity
status: Done
assignee:
  - '@claude'
created_date: '2026-07-19 13:44'
updated_date: '2026-07-21 14:19'
labels:
  - search
  - move-ordering
  - quiescence
  - robustness
dependencies: []
references:
  - engine/src/ordering.rs
  - engine/src/search.rs
  - core/src/movelist.rs
parent_task_id: TASK-64
priority: medium
type: chore
ordinal: 66600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two independent hygiene defects in the move ordering module and its quiescence loader. Neither changes played strength; both are the kind of thing that misleads a future reader or converts a bug into a silent wrong answer.

Dead quiescence quiet-move paths. `QMoveLoader::load_quiets` generates quiet moves only when the position is in check (search.rs:1529-1533), and `QMoveLoader::score_quiets` scores them (search.rs:1551-1562). Neither can ever run. `quiesce` tests `in_check` and returns through `quiesce_evasions` before reaching the OrderedMoves loop (search.rs:1242-1246), so the loop is entered only when not in check and the quiet segment is always empty. The code reads as though quiescence handles check evasions through the staged picker, which it does not. That misreading is a hazard for anyone extending quiescence later, and TASK-29 and TASK-64.9 both touch this area.

Unguarded capacity. ScoredMoveList is an ArrayVec of 254 entries (ordering.rs:23) and `ArrayVec::push_val` silently ignores a push once full (movelist.rs:224-230). Overflowing the ordering buffer therefore does not fail loudly, it drops legal moves from the search. The buffer holds the hash move, queen promotions, captures, killers, quiets, and three underpromotions per queen promotion, so worst-case occupancy is about L + 3P + 3 for L legal moves and P queen promotions.

This appears unreachable in practice, and the two extremes are mutually exclusive: a high promotion count needs pawns on the seventh rank, which displaces the sliding-piece mobility that produces a high legal move count. Measured occupancy is 218 for the standard 218-move maximum-mobility position and 135 for the most promotion-heavy positions tried. The argument is sound but nothing enforces it, and any future phase addition changes the arithmetic. A debug assertion at the boundary turns a silent wrong answer into a test failure.

Also in the same module, `next_phase` and `phase` are two names for one accessor returning the same field (ordering.rs:336-338 and ordering.rs:540-542).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Quiescence move loading contains no unreachable path, and the handling of check evasions in quiescence is evident without tracing the caller
- [ ] #2 Exceeding the ordering buffer capacity fails a debug assertion rather than silently dropping moves
- [ ] #3 A test records the worst-case occupancy argument for the ordering buffer, so a later phase addition that invalidates it is caught
- [ ] #4 The duplicated phase accessor is reduced to one name
- [ ] #5 Node counts at fixed depth are unchanged, confirming no behavioural change
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. AC#1: Remove the dead quiet paths from QMoveLoader (load_quiets in_check branch and score_quiets). Quiescence's staged loop is only reached when not in check, so quiets never load; check evasions route to quiesce_evasions before the loop. Document this on the QMoveLoader type so it is evident without tracing the caller.
2. AC#2: Introduce an ORDERING_BUFFER_CAPACITY const (254) and add a debug_assert in ScoredMoveList::push so exceeding capacity fails loudly instead of silently dropping legal moves via push_val.
3. AC#3: Add a test that drives the real MoveLoader through every phase for the 218-move maximum-mobility position and the most promotion-heavy positions, pinning the measured buffer occupancy so a future phase addition that invalidates the L + 3P + 3 argument is caught.
4. AC#4: Remove the unused next_phase accessor; keep phase().
5. AC#5: Confirm node counts at fixed depth are unchanged (release behaviour is untouched; the assertions are debug-only and the removed paths were dead).
<!-- SECTION:PLAN:END -->
