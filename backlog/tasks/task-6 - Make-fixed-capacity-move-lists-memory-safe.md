---
id: TASK-6
title: Make fixed-capacity move lists memory safe
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 18:55'
labels:
  - safety
  - movegen
dependencies: []
references:
  - core/src/movelist.rs
priority: high
type: bug
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The safe MoveList push path performs an unchecked write after only a debug assertion. Overflow must have deterministic safe behavior while preserving the fixed-capacity hot-path design used by move generation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Calling the safe push API at or beyond capacity cannot write out of bounds in any build profile
- [ ] #2 Overflow behavior is explicit and consistent for HotArrayVec and ArrayVec-backed move lists
- [ ] #3 Tests exercise exact-capacity and over-capacity insertion in debug and release-compatible code
- [ ] #4 Normal legal move generation retains all generated moves
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a capacity guard to HotArrayVec's safe push path so overflow is ignored consistently with ArrayVec without changing fixed-capacity storage.
2. Document the shared overflow contract and add boundary tests for exact-capacity and over-capacity insertion on both implementations.
3. Add a legal move-generation regression assertion, run formatting and workspace tests, then commit the immutable implementation and review handoff.
<!-- SECTION:PLAN:END -->
