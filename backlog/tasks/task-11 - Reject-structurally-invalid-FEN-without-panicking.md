---
id: TASK-11
title: Reject structurally invalid FEN without panicking
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:12'
labels:
  - core
  - fen
  - input
dependencies: []
references:
  - core/src/position/fen.rs
  - core/src/position/state.rs
modified_files:
  - core/src/position/fen.rs
priority: high
type: bug
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
FEN parsing can accept ranks with the wrong width and can construct positions without the kings required by State initialization, turning invalid input into panics or invalid internal state. Validate structural invariants before Position construction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every FEN rank must represent exactly eight squares, including the final rank
- [ ] #2 Missing, duplicate, or otherwise structurally invalid king data returns FenError rather than panicking
- [ ] #3 Invalid structural input never reaches State or Zobrist initialization
- [ ] #4 Regression tests cover short and long final ranks, empty boards, missing kings, and duplicate kings
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add explicit structural FEN validation for every rank and exactly one king per side before Position construction.
2. Add regression tests for short/long final ranks, empty and missing-king boards, duplicate kings, and valid parsing.
3. Run focused tests plus cargo fmt --check and cargo test --workspace, then commit an immutable implementation and record the review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented rank-width completion checks, including the previously unchecked final rank, and reject any board without exactly one king per side before Position, State, or Zobrist construction. Added panic-guarded regressions for short/long final ranks, empty boards, each missing king, and duplicate kings.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 19:12
---
Implementation handoff
Branch: task-11-reject-invalid-fen
Worktree: /Users/seabo/seaborg-worktrees/task-11-reject-invalid-fen
Base: 4e7c7089431de8122541bc430ff200beb954f2e1
Implementation target: 1cc446b88c1cfa41d8b442bcb668427ea33ea786
Resolved findings: none
Verification:
- cargo test -p core position::fen::tests: passed (4 tests)
- cargo fmt --check: passed
- cargo test --workspace: passed (core 19; engine 39 passed, 1 ignored; build metadata 5; doc tests 0)
Known failures: none
---

author: @codex
created: 2026-07-17 19:12
---
Handoff correction
The exact implementation target is 1cc446b3e6115865b803128423d3cb38eade10e0. The earlier handoff comment expanded the correct abbreviated SHA (1cc446b) incorrectly; all other handoff fields and verification results are unchanged.
---
<!-- COMMENTS:END -->
