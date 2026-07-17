---
id: TASK-6
title: Make fixed-capacity move lists memory safe
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
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
