---
id: TASK-57
title: Rewrite the transposition table around clustered verified snapshots
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 12:26'
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
Replace the existing direct-mapped transposition-table module and its probe/write API with a fresh implementation designed for correctness, concurrency, cache efficiency, strong replacement behavior, and direct reuse by a future Lazy SMP search. Backward compatibility with the current Table, Probe, WritableEntry, and packed-entry abstractions is not required. Preserve only proven engine conventions that remain appropriate, such as lock-free shared access, compact move representation, position-relative mate scores, configurable memory sizing, and cheap administrative invalidation. One allocation must be safely shared by arbitrary search workers without worker ownership or coordination on the probe/store hot path. The new API must return an immutable verified hit snapshot independently from the slot selected for replacement, so concurrent mutation cannot change the meaning of an already-consumed result.
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
- [ ] #11 The table is Send + Sync by construction and supports one immutable allocation shared through Arc; probe, replacement selection, store, and telemetry operate through shared references with no worker-exclusive table state
- [ ] #12 Probe and store are bounded lock-free operations on supported targets: no mutexes, read-write locks, spin locks, blocking coordination, or unbounded compare-exchange retry loops occur on the search hot path, and the native atomic target requirement or deliberate fallback policy is explicit
- [ ] #13 If an entry spans multiple atomic words, its publication and validation protocol, memory orderings, and bounded retry behavior are documented and tested so readers can never observe a hybrid entry
- [ ] #14 Concurrent races may discard or replace useful information but can never invent information; replacement is worker-agnostic so every worker can consume every other worker’s valid entries without permanent partitioning or ownership
- [ ] #15 Cluster alignment, false sharing, replacement contention, and observational hashfull behavior are exercised under representative multi-worker load as well as single-thread benchmarks
- [ ] #16 Deterministic or model-based concurrency tests cover adverse probe-versus-replace schedules, competing writers, administrative quiescence boundaries, and generation or age wrap
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rewrite engine/src/tt.rs from scratch; update the search call sites to the new API.

1. Layout (AC#1, #15). `Slot` = two `AtomicU64` (16 bytes). `Cluster` = `#[repr(C, align(64))]` array of 4 slots = exactly one 64-byte cache line, so a cluster probe touches one line and clusters never straddle two. `Table { clusters: Box<[Cluster]>, mask, age: AtomicU8 }`.

2. Identity and snapshot protocol (AC#2, #3, #13). Each slot stores `key ^ data` and `data`. A probe accepts a slot only when `w0 ^ w1 == key` and the data word's occupied bit is set, so the full 64-bit Zobrist key is verified rather than a truncated signature: accidental acceptance needs a genuine 64-bit key collision, not a 1-in-2^16 signature coincidence, and this holds identically for move-less and move-bearing entries (no move-legality proof of identity). The same XOR check validates the pair, so a reader can never consume a hybrid of two writes except on a 64-bit coincidence. Both words are Relaxed: the entry is self-contained, nothing else is published, and the validation is value-based so load/store reordering cannot defeat it. Document the write order (data, then key^data) and the bounded, retry-free reader.

3. Packed data word (AC#9). move 16 | score 16 | depth 8 | bound 2 | age 6 | reserved 15 (zero) | occupied 1. Zeroed memory is therefore empty by construction. Explicit invariants documented; `PackedMove`, position-relative mate scores and lock-free shared access are retained from the current module.

4. API (AC#2, #11, #12). `probe(&self, key: u64) -> Option<Snapshot>` returns an immutable value type, with no borrow of the slot, so replacement after a probe cannot change an already-consumed result. `store(&self, key, score, depth, bound, mov)` selects its own victim at store time, independently of any probe. Both take `&self`, are bounded (one cluster scan, no CAS, no locks, no retry loop), and there is no worker-exclusive state, so one `Arc<Table>` serves arbitrary workers. Taking a raw key rather than a `&Position` is what makes adversarial index/key tests expressible.

5. Replacement (AC#4, #14). A same-key slot is updated in place, keeping the existing move when the new one is null and declining to overwrite a strictly deeper same-age entry unless the new bound is an Exact upgrade. Otherwise the victim minimises `depth + 4*(bound == Exact) - 8*relative_age`, with empty slots always chosen first and ties broken by lowest slot index. Worker-agnostic: no slot is ever reserved for a writer, so any worker consumes any other's entries.

6. Age and invalidation (AC#6, #7). `clear(&mut self)` physically zeroes the table, which makes invalidation exact and removes the generation-wrap hazard entirely; `&mut self` plus `Arc::get_mut` in `SearchEngine` turns the current comment-only 'workers must not call this' rule into a boundary the type system enforces (both existing callers already join the worker first). Age is a separate 6-bit counter advanced once per root search via `&self`; it wraps freely because it only orders replacement priority and can never invalidate an entry. Sizing uses checked arithmetic and rounds the cluster count *down* to a power of two, so the allocation never exceeds the advertised limit (the current code rounds to nearest and a 100MB request allocates 128MB); size 0 yields one cluster.

7. Telemetry (AC#7). `hashfull` samples clusters on a stride across the whole table instead of the first 1000 entries, which currently panics for any table smaller than 1000 entries and only ever observes one contiguous region.

8. Search integration. `Search` probes once into a `Snapshot` and stores at Step 24 through `store`; quiescence Step 3/4 reads the snapshot. `Tracer`'s `hash_clash` becomes `hash_miss`, since with full-key verification 'same slot, different position' is no longer the observable event.

9. Tests (AC#5, #10, #16). Packing round-trip and field invariants; cluster selection and alignment; replacement priority incl. deeper-exact survival and same-key update; sizing boundaries incl. 0, 1, non-power-of-two and overflow; hashfull on a one-cluster table and across capacities; deterministic replacement between probe and consumption; a different key sharing the cluster index; hand-constructed torn word pairs rejected; age wrap; clear. Concurrency: multi-writer/multi-reader stress where each key's data is a known function of the key, asserting no accepted snapshot ever carries invented data.

10. Measurement (AC#8, #15). Add a `benches/tt.rs` Criterion harness for large-table construction, clear, single-thread probe/store throughput and a multi-worker mixed load; record results and retained lifecycle costs in BENCHMARKS.md, measured round-robin against the base commit.
<!-- SECTION:PLAN:END -->
