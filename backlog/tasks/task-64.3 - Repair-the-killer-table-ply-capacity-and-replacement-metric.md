---
id: TASK-64.3
title: Repair the killer table ply capacity and replacement metric
status: To Do
assignee: []
created_date: '2026-07-19 13:31'
labels:
  - search
  - move-ordering
dependencies:
  - TASK-64.1
references:
  - engine/src/killer.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: bug
ordinal: 66000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The killer table has two independent defects: it is sized for far fewer plies than the search can reach, and its replacement policy ranks slots by a counter that does not measure what the policy needs.

Capacity. The table is constructed with 20 plies (`KillerTable::new(20)` at search.rs:508) while MAX_DEPTH is 255 (search.rs:26). `KillerTable::probe` returns `(None, None)` for any draft beyond its length (killer.rs:37-42), so killers silently stop being available past ply 20 rather than failing loudly. Extensions and reductions will make the reachable ply range harder to reason about, not easier.

Replacement metric. Each slot carries a usize alongside the move (killer.rs:12-15). `probe` increments that counter for every move it finds merely legal in the current position (killer.rs:46-60), and `store` evicts whichever slot has the lower count (killer.rs:65-84). The counter therefore measures how often a move was offered, not how often it produced a cutoff. A quiet move that happens to be legal across many positions and never causes a cutoff accumulates a high count and outlives a genuinely effective killer. The counter is also used to reorder the two returned moves (killer.rs:56-59), so the same wrong signal decides which killer is tried first.

The conventional alternative is a shift-down on store, where the incoming move takes the first slot and the previous first slot moves to the second. It is simpler than the present scheme and does not require a counter at all.

This task depends on the ply refactor because the correct capacity and the correct index are both expressed in terms of ply, and re-sizing the table against a derived draft would need redoing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The killer table covers the full ply range the search can reach, or a documented cap with an explicit and tested behaviour at the boundary
- [ ] #2 Slot replacement no longer ranks candidates by how often they were offered
- [ ] #3 The order in which the two killers are yielded is determined by a documented policy that reflects cutoff usefulness
- [ ] #4 A test asserts that a killer stored at a deep ply is retrievable at that ply
- [ ] #5 A test asserts that a frequently-legal but never-cutting move does not evict a recently successful killer
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
