---
id: TASK-12
title: Repair transposition-table reuse and mate-score semantics
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - search
  - tt
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/tt.rs
priority: high
type: bug
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search unconditionally clears the shared transposition table because of a known PVS interaction, preventing reuse and undermining concurrent workers. Mate scores also need ply-aware storage and retrieval so transpositions preserve distance-to-mate ordering.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Starting a normal search does not unconditionally invalidate the shared transposition table
- [ ] #2 New-game and explicit clear operations have documented ownership and generation behavior
- [ ] #3 Mate scores are encoded and decoded relative to ply so transposed positions preserve mate distance
- [ ] #4 Concurrent search workers do not invalidate one another through table generation changes
- [ ] #5 Tests cover reuse across searches, explicit clear, transposed mate scores at different plies, and concurrent probes
<!-- AC:END -->
