---
id: TASK-16
title: 'Fix perft depth-zero, verbose, and check statistics'
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - perft
  - cli
dependencies: []
references:
  - engine/src/perft.rs
  - src/perft.rs
priority: medium
type: bug
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Perft depth zero underflows into recursion, the CLI verbose flag is ignored, and check statistics count only double checks. Make the CLI and library edge cases consistent and accurate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Perft at depth zero returns exactly one node without recursion or panic
- [ ] #2 The CLI verbose flag enables the documented detailed counters and timing output
- [ ] #3 The check counter includes every checking leaf while double checks are not substituted for all checks
- [ ] #4 Divide rejects or handles depth zero consistently with normal perft
- [ ] #5 Tests cover depth zero and known detailed perft statistics
<!-- AC:END -->
