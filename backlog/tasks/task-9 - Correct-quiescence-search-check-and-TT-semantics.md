---
id: TASK-9
title: Correct quiescence search check and TT semantics
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:07'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Refactor quiescence TT probing so valid hit scores are applied only as depth-qualified alpha-beta bounds, never as stand-pat evaluations.
2. Separate in-check quiescence from stand-pat logic and search every legal evasion, returning mate when none exist.
3. Add focused regression tests for quiet evasions, horizon mate, TT exact/lower/upper and insufficient-depth behavior, plus window invariants.
4. Run formatting and workspace tests, commit the implementation, then record an immutable In Review handoff.
<!-- SECTION:PLAN:END -->
