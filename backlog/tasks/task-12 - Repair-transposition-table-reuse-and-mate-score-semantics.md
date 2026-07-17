---
id: TASK-12
title: Repair transposition-table reuse and mate-score semantics
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 21:40'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Define ply-aware transposition-table score encoding/decoding and apply it consistently to main-search and quiescence probes/writes.
2. Remove automatic table invalidation from Search::run; expose documented SearchEngine clear/new-game ownership and wire UCI/game reset operations to it.
3. Add regression tests for sequential reuse, explicit/new-game invalidation, mate scores probed at different plies, and concurrent workers retaining the same generation.
4. Run focused tests, cargo fmt --check, and cargo test --workspace; commit implementation and record the immutable In Review handoff.
<!-- SECTION:PLAN:END -->
