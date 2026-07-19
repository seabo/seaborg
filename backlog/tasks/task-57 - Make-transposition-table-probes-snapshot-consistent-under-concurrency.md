---
id: TASK-57
title: Rewrite the transposition table around clustered verified snapshots
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 13:12'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
`engine/src/tt.rs` was rewritten; `Table`, `Probe`, `WritableEntry` and the packed `Entry` are gone. Call sites in `engine/src/search.rs` moved to the new API, and `Tracer::hash_clash` became `hash_miss`.

Evidence per acceptance criterion (all test names are in `engine/src/tt.rs` unless noted):

- AC#1 cluster layout: `Cluster` is `#[repr(C, align(64))]` over four 16-byte slots. `cluster_is_one_cache_line_and_slots_fill_it`, `clusters_are_cache_line_aligned_in_the_allocation`, `a_cluster_holds_several_distinct_keys_at_once`.
- AC#2 snapshot identity: `probe` returns an owned `Snapshot`, not a borrow. `a_replacement_between_probe_and_consumption_cannot_change_the_snapshot` drives the adverse schedule deterministically by evicting the whole cluster and asserting the already-consumed value is unchanged.
- AC#3 verification strength: full 64-bit key, not a signature. The module docs quantify why at a 1GB table and ~10^9 probes (a 16-bit signature admits ~10^4 wrong-position acceptances per search, and move legality does not cover move-less entries). `no_single_bit_key_variation_is_accepted`, `a_different_key_sharing_a_cluster_index_is_never_accepted`, `a_move_less_entry_is_stored_and_found`.
- AC#4 replacement: `a_shallow_entry_does_not_evict_a_deeper_exact_one`, `an_exact_bound_outranks_an_equally_deep_inexact_one`, `an_older_entry_is_evicted_before_a_deeper_current_one`, `a_same_key_store_updates_in_place_rather_than_consuming_a_slot`, `a_shallower_same_key_store_does_not_erase_a_deeper_result`, `a_shallower_same_key_store_still_lands_when_it_upgrades_the_bound`, `a_stale_deeper_same_key_entry_is_refreshed_by_a_new_search`, `a_move_less_update_keeps_the_move_already_recorded`, `empty_slots_are_filled_before_anything_is_evicted`.
- AC#5 lock-free, race-free: relaxed loads/stores only, no CAS or retry on probe/store. `a_replacement_between_probe_and_consumption_cannot_change_the_snapshot` and `a_different_key_sharing_a_cluster_index_is_never_accepted` are the two deterministic cases the criterion names.
- AC#6 age and invalidation: clearing is physical and takes `&mut self`, so `Arc::get_mut` in `SearchEngine::clear_hash` makes the ownership boundary a type-system property rather than a comment. `clear_discards_every_entry_and_resets_the_age`, `age_wraps_without_invalidating_entries`, `relative_age_is_wrapping_and_never_negative`, `concurrent_age_advances_never_invalidate_entries`, and `searches_reuse_the_shared_table_until_the_owner_clears_it` in search.rs.
- AC#7 sizing and telemetry: saturating arithmetic, cluster count rounds *down*. `sizing_rounds_down_and_never_exceeds_the_request` asserts the allocation never exceeds the request and is the largest that fits; `sizing_boundaries_degrade_to_one_cluster_and_saturate`; `hashfull_is_total_for_every_supported_capacity` (the old code panicked below 1000 entries), `hashfull_samples_the_whole_table_not_a_prefix`, `hashfull_rises_with_occupancy`.
- AC#8/AC#15 measurement: `benches/tt.rs` plus the BENCHMARKS.md 'Transposition table' section. Nodes to depth 10 fall 2.5% (exact, reproduces identically); per-node throughput falls ~3%; time to depth is level within drift. Retained costs are documented, including the linear clear as a deliberate regression against the old constant-time generation bump.
- AC#9 semantics: packed-field invariants are documented at the layout constants and asserted in `Snapshot::from_data`/`Slot::store`. `reserved_bits_stay_zero_and_empty_slots_are_all_zero`, `round_trips_every_packed_field`. No legacy API remains.
- AC#10 test coverage: the 34 tests in the module.
- AC#11 shared-reference API: `probe`, `store` and `hashfull` all take `&self`; `table_is_send_and_sync`, `every_worker_can_consume_every_other_workers_entries`.
- AC#12 bounded lock-freedom: no mutex, no CAS, no retry on either hot-path operation. The native-atomic requirement is explicit and enforced by `compile_error!` under `cfg(not(target_has_atomic = "64"))` — there is deliberately no lock-based fallback.
- AC#13 multi-word protocol: documented on `Slot` (write order, relaxed orderings and why they suffice, bounded retry-free reads). `a_hand_constructed_torn_pair_is_rejected` builds the hybrid by hand rather than hoping to race into one.
- AC#14 never invent information: `concurrent_writers_and_readers_never_invent_an_entry` makes each key's score a known function of the key and asserts every accepted snapshot satisfies it, so a race may lose an entry but cannot fabricate one. `every_worker_can_consume_every_other_workers_entries` covers worker-agnostic replacement.
- AC#16 adverse schedules: the deterministic tests above for probe-versus-replace, torn publication, and age wrap; the threaded tests for competing writers; `searches_reuse_the_shared_table_until_the_owner_clears_it` for the administrative quiescence boundary.

Deliberate decisions the reviewer should weigh:

1. Entry width doubled to 16 bytes to carry the full key, halving entries per megabyte. This is the central trade of the task and it is measured, not assumed: `hashfull` reports 607 against the base's 294 at depth 10 and the same hash size, yet the node count still falls 2.5%.
2. Clearing became linear where it was constant-time. Justified in BENCHMARKS.md: the generation bump left stale entries physically present, needed a table walk on wrap anyway, and could revive an entry when the counter lapped.
3. `Table::probe`/`store` take a raw `u64` key rather than `&Position`. Positions cannot be constructed with chosen keys, so a `&Position` API would have made the adversarial index/key tests inexpressible.
4. `SearchEngine::clear_hash` now takes `&mut self` and panics if a search still holds the table. Both existing callers already join their worker first, so the boundary is met; the panic is deliberate, since the alternative is silently clearing under a live worker.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 12:55
---
Implementation handoff
Branch: task-57-tt-clustered-snapshots
Worktree: /Users/seabo/seaborg-worktrees/task-57-tt-clustered-snapshots
Base: 9b7bf3392ccd4adf43effdaa990bacb45c40a15c
Implementation target: fe46d6d81bde8e685f0c69b174805fd629b0c82d
Resolved findings: none (first implementation attempt)
Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo test --workspace: 279 passed, 0 failed, 2 ignored (both pre-existing #[ignore])
- cargo bench --bench tt: recorded in BENCHMARKS.md
- cargo bench --bench search, round-robin base vs target over 3 rounds: level (42.48 us vs 42.11 us, best of three, with deadline)
- go depth 10 from startpos, round-robin base vs target over 9 rounds: identical score and PV at every depth; 4,883,269 -> 4,762,311 nodes (2.5% fewer, exact and reproducible); best time 882 ms vs 891 ms, inside the machine's drift band
Known failures: none

Note for the reviewer: master has moved since this branch was cut. TASK-55 merged as 909d54e after this base commit, so the branch is behind primary. Nothing here touches the search-documentation change TASK-55 made, but the merge gate will be integrating against a newer tip than the measured base.
---

author: @codex
created: 2026-07-19 13:12
---
Note for the reviewer: key width was queried, and the density arithmetic in my handoff notes was understated. Corrected below. No code changed; the implementation target remains fe46d6d81bde8e685f0c69b174805fd629b0c82d.

AC#3 permits either a signature or a full key, provided accidental acceptance is negligible. I chose the full 64-bit key. A 32-bit key is the obvious cheaper point on that curve and is worth an explicit opinion from someone who did not write this.

The entry is key + data, and the data is 48 bits with nothing meaningful to give back:

  move 16 (15 needed plus the null flag) + score 16 (+/-30,000 range)
  + depth 8 (MAX_DEPTH is 255, so all eight are real) + bound 2 + age 6 = 48

That fixes the available layouts:

  key 64 (as implemented): 16-byte entry, 4 per 64-byte cluster
  key 32:                  12-byte entry, 5 per 64-byte cluster (1.25x)
  key 16 (previous table):  8-byte entry, 8 per 64-byte cluster (2x)

So a 32-bit key buys about 25% more entries, not 2x. Doubling capacity needs an 8-byte entry, which forces the key back to 16 bits and reinstates exactly the failure mode this task removed. The 12-byte variant also needs the cluster restructured into separate key and data arrays, because a u32 next to a u64 pads back to 16 bytes.

Collision rates, at a 1GB table and roughly 10^9 probes per search, four slot comparisons per probe:

  key 16: order 10^4-10^5 wrong-position acceptances per search, only partly filtered by move
          legality, and not filtered at all for the move-less bounds the search cuts off against
  key 32: order 1 per search, comparable to genuine Zobrist collisions
  key 64: effectively zero

The case for switching now looks weak to me, but I am not the right person to close it:

- Time to depth is already level and node count already improved 2.5%. A further 25% of entries
  plausibly buys around 1% more nodes, which is below the noise floor this machine measured at
  (individual runs of one binary ranged 882-1510 ms).
- It is a re-implementation, not a constant change: new packing, structure-of-arrays clusters, the
  tear protocol re-verified against a 32-bit check, and the full benchmark set re-run.

There is a larger lever than key width, and it is a policy question rather than an optimisation.
Stockfish stores a 16-bit key in 10-byte entries and tolerates rare torn reads, which yields 6
entries per cluster - better than either option above. This task deliberately went the other way:
AC#2 and AC#13 require that a hybrid entry can never be consumed. Whether that guarantee is worth
its density is a decision for a human, and reversing it should not be smuggled into this task.

Flagged rather than actioned, and no follow-up task created: the scope call is not mine to make.
---
<!-- COMMENTS:END -->
