---
id: TASK-16
title: 'Fix perft depth-zero, verbose, and check statistics'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 23:19'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Fix depth-zero in Perft::perft_inner: return exactly one node before any movegen/recursion (avoids usize underflow on depth-1).
2. Wire the CLI --verbose flag (src/perft.rs) to collect_detailed_data so documented detailed counters + timing print; keep divide behaviour consistent.
3. Fix check counter in handle_leaf: count every checking leaf via Position::in_check() instead of in_double_check(); keep checkmate counter.
4. Make Perft::divide handle depth zero consistently with perft (return one node, no panic) instead of assert.
5. Add tests: depth-zero returns 1 node for perft and divide; detailed statistics (nodes/captures/ep/castles/promotions/checks/checkmates) against known chessprogramming.org values for start position and Kiwipete.
6. Run cargo fmt --check and cargo test --workspace.
<!-- SECTION:PLAN:END -->
