---
id: TASK-4
title: Make transposition-table generation invariants explicit
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 16:44'
updated_date: '2026-07-17 18:48'
labels: []
dependencies: []
references:
  - engine/src/tt.rs
modified_files:
  - engine/src/tt.rs
type: bug
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Transposition-table generations are six-bit epoch identifiers, with 0 reserved as the empty-entry sentinel and live generations cycling through 1..=63. The current GenBound documentation and test describe out-of-range values as being silently truncated, while the debug implementation rejects them. Replace this contradictory behavior with a representation and API that cannot silently alias invalid generations to empty or live epochs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Live transposition-table entries can only use generation identifiers in the range 1..=63
- [ ] #2 Generation 0 remains reserved exclusively for the empty-entry representation
- [ ] #3 Out-of-range generation input is rejected consistently in debug and release builds rather than truncated or aliased
- [ ] #4 Generation wraparound physically invalidates old entries before an epoch identifier is reused
- [ ] #5 Documentation and tests describe and verify empty, valid, invalid, and wraparound generation behavior
- [ ] #6 cargo test --workspace passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Introduce a checked Generation value type representing only live epochs 1..=63, and make GenBound construction require it while retaining raw zero solely for default empty entries.
2. Route table generation loads and writable entries through Generation, and change wraparound clearing so storage is physically zeroed before generation 1 is published again.
3. Replace contradictory generation tests with coverage for empty encoding, valid boundaries, rejected zero/out-of-range inputs, and physical invalidation on wrap.
4. Run cargo fmt --check and cargo test --workspace, then record the immutable implementation handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a checked Generation type for live epochs 1..=63 and made GenBound construction require it. Generation zero remains available only through the default empty packed representation. Reworked Table::clear so wraparound zeroes every slot before publishing generation 1, and added tests for invalid inputs, valid bounds, empty encoding, normal invalidation, and physical wrap invalidation.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 18:48
---
Implementation handoff
Branch: task-4-tt-generation-invariants
Worktree: /Users/seabo/seaborg-worktrees/task-4-tt-generation-invariants
Base: ff4276b3b26928053f042776231fc6a9e8d4c163
Implementation target: c4de7e4f35739315344b0ee06250a2f4d215dab5
Resolved findings: none
Verification:
- cargo test -p engine tt::tests: passed (7 passed, 1 ignored)
- cargo fmt --check: passed
- cargo test --workspace: passed
Known failures: none
---
<!-- COMMENTS:END -->
