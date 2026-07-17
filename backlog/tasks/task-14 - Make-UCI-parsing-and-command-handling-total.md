---
id: TASK-14
title: Make UCI parsing and command handling total
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - uci
  - input
dependencies: []
references:
  - engine/src/uci.rs
  - engine/src/engine.rs
priority: high
type: bug
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The UCI parser contains panic paths and unchecked numeric narrowing, while several successfully parsed standard commands fall through as unimplemented. Make parsing total and ensure supported and unsupported commands have protocol-safe outcomes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No token sequence can reach todo, unwrap, or another parser panic
- [ ] #2 Depth and numeric parameters are range checked without truncation
- [ ] #3 Trailing tokens are handled consistently for all commands
- [ ] #4 Every parsed standard UCI command is either implemented or rejected without emitting non-protocol stdout
- [ ] #5 Parser and driver tests cover reserved standalone tokens, oversized numbers, malformed commands, setoption, and ucinewgame
<!-- AC:END -->
