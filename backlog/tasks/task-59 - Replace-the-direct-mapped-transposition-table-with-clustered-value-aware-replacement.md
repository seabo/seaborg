---
id: TASK-59
title: >-
  Replace the direct-mapped transposition table with clustered value-aware
  replacement
status: To Do
assignee: []
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 00:06'
labels:
  - transposition-table
  - performance
  - search
  - architecture
dependencies:
  - TASK-57
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: high
type: enhancement
ordinal: 58000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The table currently gives every key one slot and overwrites that slot after every completed main-search node. Shallow bounds and unrelated clashes can therefore evict deep exact results, and the generation is usable only as a global-clear sentinel rather than replacement age. Move to a compact clustered layout with a deterministic replacement policy that preserves valuable entries while retaining atomic shared-table access.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each indexed bucket offers multiple candidate entries without increasing the requested table allocation beyond its documented sizing policy
- [ ] #2 Replacement decisions distinguish same-key updates from clashes and account for depth, bound quality, and age so shallow or weak entries do not unconditionally evict deeper exact results
- [ ] #3 Generation or age semantics support replacement and explicit new-game invalidation, including safe wrap behavior
- [ ] #4 Hash allocation uses checked integer sizing with defined zero and boundary behavior, and normal requests do not exceed the advertised memory limit
- [ ] #5 hashfull is safe for every supported capacity and reports a per-mille estimate of current usable occupancy
- [ ] #6 Dead or misleading entry APIs and comments are removed, and deterministic tests cover cluster selection, replacement priorities, sizing boundaries, generation wrap, and small-table telemetry
- [ ] #7 A reproducible benchmark or node-count comparison demonstrates the clustered design does not materially regress probe throughput and records its effect on search efficiency
- [ ] #8 The ownership contract for administrative clears is enforced by the API or deterministically tested so an active search cannot accidentally have its live generation invalidated
- [ ] #9 hashfull samples occupancy robustly rather than assuming one fixed contiguous region is representative, while remaining cheap enough for periodic UCI reporting
- [ ] #10 Large-table construction and generation-wrap clearing latency are measured; avoidable stalls are removed or the retained lifecycle cost and safe invocation boundary are documented
- [ ] #11 Packed-field helpers and names make null-move presence, generation, and bound invariants clear without redundant or unfinished APIs
<!-- AC:END -->
