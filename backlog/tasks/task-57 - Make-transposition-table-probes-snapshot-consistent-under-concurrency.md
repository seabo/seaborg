---
id: TASK-57
title: Make transposition-table probes snapshot-consistent under concurrency
status: To Do
assignee: []
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 00:06'
labels:
  - transposition-table
  - search
  - concurrency
  - correctness
dependencies: []
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: high
type: bug
ordinal: 56000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A probe currently signature-checks one AtomicU64 snapshot and returns a slot handle, after which search reloads the slot and checks only its generation. A sibling search can replace that slot between the two loads, allowing the original position to consume another position’s depth, bound, or score. Make the result consumed by search demonstrably belong to the key that was probed, while retaining packed atomic lockless storage and shared-table operation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Main search and quiescence never consume depth, bound, score, or move data from a slot snapshot that was not verified for the requested position key
- [ ] #2 A deterministic regression test replaces a same-generation slot between probe and consumption and proves that the replacement is not treated as a hit for the original position
- [ ] #3 Concurrent probes and competing writers remain data-race-free without locks or torn-entry reads
- [ ] #4 Probe and slot APIs and documentation accurately describe snapshot and mutation semantics; they do not claim unique access
- [ ] #5 Verification strength is explicitly assessed against realistic table sizes and search volumes; the chosen signature or full-key scheme makes accidental score acceptance suitably negligible and is applied consistently to move-less as well as move-bearing entries
- [ ] #6 Tests exercise a matching index and signature for a different key and demonstrate the documented collision policy without relying on stored-move legality as proof of identity
<!-- AC:END -->
