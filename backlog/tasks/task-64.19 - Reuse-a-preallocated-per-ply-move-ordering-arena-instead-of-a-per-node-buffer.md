---
id: TASK-64.19
title: Reuse a preallocated per-ply move ordering arena instead of a per-node buffer
status: To Do
assignee: []
created_date: '2026-07-19 13:44'
labels:
  - search
  - move-ordering
  - performance
  - architecture
dependencies:
  - TASK-64.1
  - TASK-64.17
references:
  - engine/src/ordering.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: low
type: enhancement
ordinal: 82000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
OrderedMoves is constructed fresh at every interior node and lives as a stack local for that node. Replace it with an arena allocated once per search and indexed by ply.

Current state. search.rs:801 constructs one per node in the main search and search.rs:1248 constructs one per node in quiescence. Measured size is 2152 bytes, so a search reaching 60 ply carries roughly 130KB of live ordering buffers on the recursion stack, and every node pays construction and teardown of a 254-entry buffer even though most nodes cut off during the first two phases and touch only a handful of entries.

This is not currently a bottleneck and should not be presented as one, which is why it is Low priority. The reasons to do it are that it removes a per-node cost scaling with nothing useful, it bounds stack growth explicitly rather than incidentally, and it is the conventional arrangement in engines that later add multi-threaded search, where each thread wants preallocated per-ply state rather than deep stacks.

Relationship to other work. TASK-64.1 introduces a per-ply search stack holding static evaluation, the move played and the excluded move. The ordering buffer is naturally a further slot on that same stack, and doing this independently would produce a second per-ply structure with a second indexing convention, which is precisely what TASK-64.1 exists to prevent. TASK-64.17 changes the Entry layout and the segment representation, and sizing or initialising an arena of the current layout would be redone once that lands. Hence both dependencies.

TASK-64.16 adds Lazy SMP. Whatever arrangement lands here must be per-thread, and that boundary is easier to get right before threads exist than after.

An open question to settle and record: whether quiescence shares the same arena as the main search, indexed by a combined ply, or holds a separate one. Quiescence can descend well past the main search ply, so the sizing decision is not the same for both.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Move ordering buffers are allocated once per search rather than once per node, and are indexed by ply
- [ ] #2 The arena is per-thread, or the ownership arrangement that will make it per-thread under Lazy SMP is documented
- [ ] #3 The decision on whether quiescence shares the arena with the main search is recorded with rationale, including how the arena is sized against the reachable quiescence ply
- [ ] #4 Peak stack usage of a deep search is reduced, with before and after figures recorded
- [ ] #5 The search benchmark is recorded before and after and shows no regression
- [ ] #6 Node counts at fixed depth are unchanged, confirming no behavioural change
<!-- AC:END -->
