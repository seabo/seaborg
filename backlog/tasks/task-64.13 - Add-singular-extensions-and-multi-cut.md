---
id: TASK-64.13
title: Add singular extensions and multi-cut
status: To Do
assignee: []
created_date: '2026-07-19 13:33'
updated_date: '2026-07-22 02:55'
labels:
  - search
  - extensions
dependencies:
  - TASK-64.1
  - TASK-51
references:
  - engine/src/search.rs
parent_task_id: TASK-64
priority: low
type: feature
ordinal: 76000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add singular extensions: when the transposition-table move appears to be the only move that holds a position, search it deeper. Multi-cut is the complementary case, where several moves beat beta in a reduced search and the node can be pruned instead.

This is the most sophisticated item in the programme and is sequenced last among the search features for that reason. It is also the one with the strictest structural prerequisites, which is why it is scheduled after both the search-stack refactor and the general extension and reduction work.

Mechanism. At a node with a sufficiently deep transposition-table entry, re-search the remaining moves at reduced depth with a window just below the stored score, excluding the stored move. If they all fail low, the stored move is singular and is extended. The exclusion is the structural requirement: the re-search must be able to skip one specific move, and that excluded move must be recorded per ply where the recursive call can see it. There is nowhere to record it today, which is why this depends on the search stack.

It depends on TASK-51 because singular extensions are an extension policy, and TASK-51 establishes the extension and reduction framework at search steps 16 and 17 that this builds on. Applying singular extensions to a search with no other extension mechanism would mean building that framework here instead.

An interaction to watch: the re-search runs at the same node and shares its transposition-table slot. The stored entry that triggered the singular test must not be overwritten by the re-search in a way that invalidates the test, and no re-search may publish an entry for the node under its artificial window. The TASK-46 guarantee that incomplete subtrees cannot publish scores is the relevant precedent.

Multi-cut may be delivered with this or deferred with a recorded decision; it reuses the same reduced re-search and is conventionally cheap to add once the exclusion mechanism exists.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A move can be excluded from a search at a given ply, and the exclusion is visible to the recursive call
- [ ] #2 Singular extensions are applied under documented depth and bound conditions and are disabled at the root
- [ ] #3 The singular re-search cannot overwrite or corrupt the transposition-table entry that triggered it, and publishes no entry for the node under its artificial window
- [ ] #4 Multi-cut is implemented, or a decision to defer it is recorded with rationale
- [ ] #5 The reported principal variation remains legal under extension, verified against the TASK-36 regression coverage
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
