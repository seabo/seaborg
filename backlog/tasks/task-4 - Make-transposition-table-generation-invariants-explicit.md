---
id: TASK-4
title: Make transposition-table generation invariants explicit
status: To Do
assignee: []
created_date: '2026-07-17 16:44'
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
