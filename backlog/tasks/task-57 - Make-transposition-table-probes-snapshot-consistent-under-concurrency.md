---
id: TASK-57
title: Rewrite the transposition table around clustered verified snapshots
status: To Do
assignee: []
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 00:25'
labels:
  - transposition-table
  - performance
  - search
  - architecture
  - concurrency
  - correctness
dependencies:
  - TASK-58
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: high
type: enhancement
ordinal: 56000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the existing direct-mapped transposition-table module and its probe/write API with a fresh implementation designed for correctness, concurrency, cache efficiency, and strong replacement behavior. Backward compatibility with the current Table, Probe, WritableEntry, and packed-entry abstractions is not required. Preserve only proven engine conventions that remain appropriate, such as lock-free shared access, compact move representation, position-relative mate scores, configurable memory sizing, and cheap administrative invalidation. The new API must return an immutable verified hit snapshot independently from the slot selected for replacement, so concurrent mutation cannot change the meaning of an already-consumed result.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each indexed cache-conscious bucket provides multiple candidate entries within a documented compact layout and within the configured memory limit
- [ ] #2 A probe returns one immutable snapshot whose identity, move, depth, bound, and score all came from the same atomic state; a concurrent replacement cannot turn a verified hit into data for another key
- [ ] #3 Verification strength is assessed against realistic table sizes and search volumes; the chosen signature or full-key scheme makes accidental score acceptance suitably negligible for move-less and move-bearing entries without using move legality as proof of identity
- [ ] #4 Replacement distinguishes same-key updates from clashes and accounts for depth, bound quality, and age so shallow or weak entries do not unconditionally evict deeper exact results
- [ ] #5 Concurrent probes and competing writers remain lock-free and data-race-free without torn-entry reads, with deterministic tests for replacement between probe and consumption and for a different key sharing index and signature
- [ ] #6 Age and administrative invalidation semantics support explicit new-game clearing and safe wrap behavior; the API enforces or deterministically tests the ownership boundary that prevents active searches from being invalidated accidentally
- [ ] #7 Allocation uses checked integer sizing with defined boundary behavior, does not exceed the advertised memory limit, and hashfull safely reports a robust per-mille occupancy estimate for every supported capacity without relying on one fixed contiguous sample
- [ ] #8 Large-table construction, clearing or wrap, probe throughput, and search efficiency are measured; avoidable stalls or material regressions are removed and retained lifecycle costs are documented
- [ ] #9 The replacement module has clear snapshot and mutation semantics, explicit packed-field invariants, and no redundant, misleading, or unfinished legacy APIs
- [ ] #10 Tests cover entry packing, cluster selection, replacement priorities, sizing boundaries, small-table telemetry, concurrent access, administrative invalidation, and generation or age wrap
<!-- AC:END -->
