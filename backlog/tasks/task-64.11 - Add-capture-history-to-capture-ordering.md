---
id: TASK-64.11
title: Add capture history to capture ordering
status: To Do
assignee: []
created_date: '2026-07-19 13:33'
updated_date: '2026-07-19 13:44'
labels:
  - search
  - move-ordering
  - see
dependencies:
  - TASK-64.2
  - TASK-64.17
references:
  - engine/src/ordering.rs
  - engine/src/history.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 74000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Captures are ordered purely by static exchange evaluation, with no learned component. Add a capture history table so that captures the search has found to cause cutoffs are tried earlier among captures of equal material outcome.

Current state. Both capture scorers assign the raw SEE value as the ordering score (search.rs:1472-1486 and search.rs:1532-1546). The phase machinery then splits captures into GoodCaptures (SEE greater than zero), EqualCaptures (SEE equal to zero) and BadCaptures (SEE less than zero) via predicate filters over one generated segment (ordering.rs:580-624).

SEE answers only whether a capture wins material on that square. It cannot distinguish between several captures with identical material outcomes, and the EqualCaptures phase in particular is currently ordered arbitrarily within itself. A capture history table, conventionally keyed on moving piece, destination square and captured piece type, supplies the missing signal.

Scope note. This is deliberately separate from the counter-move and continuation history work so that the two can be measured independently; capture ordering and quiet ordering fail in different ways and a combined measurement would not attribute either. It shares the bonus, malus and aging scheme established by the history activation task, which is why it depends on it.

An open question to settle and record: whether capture history should adjust ordering within the existing SEE-derived phases only, or whether it should be able to promote a capture across a phase boundary. The former is more conservative and preserves the material-based phase guarantee that other work in this programme relies on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A capture history table is maintained and contributes to capture ordering scores
- [ ] #2 The decision on whether capture history can move a capture across an SEE-derived phase boundary is recorded and implemented
- [ ] #3 Bonus, malus and aging follow the scheme established for plain history
- [ ] #4 A test asserts that among captures with equal SEE, one previously causing cutoffs is ordered first
- [ ] #5 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
