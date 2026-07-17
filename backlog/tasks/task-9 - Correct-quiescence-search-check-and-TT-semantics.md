---
id: TASK-9
title: Correct quiescence search check and TT semantics
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - search
  - correctness
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/tt.rs
priority: high
type: bug
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Quiescence currently allows stand-pat behavior while in check and reuses transposition-table search scores as static evaluations without sufficient bound or depth semantics. Restore legal check-evasion behavior and valid alpha-beta windows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Positions in check never return a stand-pat cutoff and search all required legal evasions
- [ ] #2 Transposition-table values are used in quiescence only when their stored depth and bound semantics justify the use
- [ ] #3 A stored search score is not substituted for a static evaluation unless it was explicitly stored as one
- [ ] #4 Quiescence never recurses with an empty or inverted alpha-beta window
- [ ] #5 Regression tests cover quiet check evasions, checkmate at the horizon, and TT hit variants
<!-- AC:END -->
