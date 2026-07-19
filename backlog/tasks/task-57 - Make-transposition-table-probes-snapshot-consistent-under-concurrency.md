---
id: TASK-57
title: Rewrite the transposition table around clustered verified snapshots
status: Done
assignee:
  - '@codex'
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 14:10'
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
- [x] #1 Each indexed cache-conscious bucket provides multiple candidate entries within a documented compact layout and within the configured memory limit
- [x] #2 A probe returns one immutable snapshot whose identity, move, depth, bound, and score all came from the same atomic state; a concurrent replacement cannot turn a verified hit into data for another key
- [x] #3 Verification strength is assessed against realistic table sizes and search volumes; the chosen signature or full-key scheme makes accidental score acceptance suitably negligible for move-less and move-bearing entries without using move legality as proof of identity
- [x] #4 Replacement distinguishes same-key updates from clashes and accounts for depth, bound quality, and age so shallow or weak entries do not unconditionally evict deeper exact results
- [x] #5 Concurrent probes and competing writers remain lock-free and data-race-free without torn-entry reads, with deterministic tests for replacement between probe and consumption and for a different key sharing index and signature
- [x] #6 Age and administrative invalidation semantics support explicit new-game clearing and safe wrap behavior; the API enforces or deterministically tests the ownership boundary that prevents active searches from being invalidated accidentally
- [x] #7 Allocation uses checked integer sizing with defined boundary behavior, does not exceed the advertised memory limit, and hashfull safely reports a robust per-mille occupancy estimate for every supported capacity without relying on one fixed contiguous sample
- [x] #8 Large-table construction, clearing or wrap, probe throughput, and search efficiency are measured; avoidable stalls or material regressions are removed and retained lifecycle costs are documented
- [x] #9 The replacement module has clear snapshot and mutation semantics, explicit packed-field invariants, and no redundant, misleading, or unfinished legacy APIs
- [x] #10 Tests cover entry packing, cluster selection, replacement priorities, sizing boundaries, small-table telemetry, concurrent access, administrative invalidation, and generation or age wrap
- [x] #11 The table is Send + Sync by construction and supports one immutable allocation shared through Arc; probe, replacement selection, store, and telemetry operate through shared references with no worker-exclusive table state
- [x] #12 Probe and store are bounded lock-free operations on supported targets: no mutexes, read-write locks, spin locks, blocking coordination, or unbounded compare-exchange retry loops occur on the search hot path, and the native atomic target requirement or deliberate fallback policy is explicit
- [x] #13 If an entry spans multiple atomic words, its publication and validation protocol, memory orderings, and bounded retry behavior are documented and tested so readers can never observe a hybrid entry
- [x] #14 Concurrent races may discard or replace useful information but can never invent information; replacement is worker-agnostic so every worker can consume every other worker’s valid entries without permanent partitioning or ownership
- [x] #15 Cluster alignment, false sharing, replacement contention, and observational hashfull behavior are exercised under representative multi-worker load as well as single-thread benchmarks
- [x] #16 Deterministic or model-based concurrency tests cover adverse probe-versus-replace schedules, competing writers, administrative quiescence boundaries, and generation or age wrap
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

author: @codex
created: 2026-07-19 13:28
---
Review attempt: 1
Reviewed branch: task-57-tt-clustered-snapshots
Reviewed implementation: fe46d6d81bde8e685f0c69b174805fd629b0c82d
Verdict: changes_requested

The implementation is sound and I found no correctness, concurrency, or
performance defect. All three blocking findings are comment defects in code that
otherwise needs no change. Substance is not in question; see "Assessment" below.

REV-1-01 [P2] Test doc comments cite acceptance criteria a reader cannot resolve
Location: engine/src/tt.rs:1074, 1100, 1249, 1300
Impact: Four test doc comments open with bare process artifacts — "AC#2/AC#5.",
  "AC#13.", "AC#14.", "AC#11/AC#12." These identify why the work was scheduled,
  not what the test proves, and become uninterpretable once the Backlog entry is
  archived. Master commit 74b53d6 ("docs: remove process-artifact references from
  code comments") removed exactly this pattern across nine files, and 4025c4b
  tightened the skills to require it, stating: "Test doc comments describe the
  behavior under test and why it matters, not which criterion demanded the test."
  Merging these would reintroduce the pattern hours after it was purged.
  Note in fairness: both commits landed on master AFTER this branch's base
  (9b7bf33), and the branch's own copy of the implement skill does not contain
  the rule. This is a standard that moved mid-flight, not a lapse.
Reproduction: grep -n "/// AC#" engine/src/tt.rs
Expected: Drop the leading criterion labels. The sentences that follow each
  prefix already stand alone and state the reason, so no rewriting is needed —
  only deletion of the four prefixes.

REV-1-02 [P2] The hashfull stride comment states a false justification
Location: engine/src/tt.rs:645-646
Impact: The comment reads "A power-of-two stride over a power-of-two cluster
  count visits `sampled` distinct clusters spread evenly across the table." The
  stride is `clusters / 250`, which is not a power of two for any cluster count
  at or above 2^14 — that is, for every table of 1MB or more, including the 16MB
  default and every size a real session uses. It is a power of two only for
  tables of 8 clusters to 8192 clusters. The sampling behaviour is correct and
  well spread regardless (stride >= 1 makes the visited indices distinct, and the
  sample reaches ~99.6% across the allocation), so this is a wrong reason
  attached to a right result, in a module where the documented reasoning is
  itself a deliverable.
Reproduction: for a 1MB table, capacity_clusters() == 16384 and
  16384 / 250 == 65, which is not a power of two. Same for every larger size:
  2^20 / 250 == 4194, 2^24 / 250 == 67108.
Expected: State the property the code actually relies on — that `stride >= 1`
  makes the `sampled` visited indices distinct, and that striding rather than
  taking a prefix is what makes the estimate representative and total down to a
  one-cluster table. Do not claim the stride is a power of two.

REV-1-03 [P3] hash_probes doc comment lost its meaning and does not parse
Location: engine/src/trace.rs:124
Impact: The doc became "The total number of hash probes, which every probe falls
  into exactly one of." The relative clause has no antecedent it can attach to —
  a probe cannot fall into a total — and the sentence no longer says what the
  function sums. The comment it replaced ("calculated as the sum of hits,
  collisions and clashes recorded") was correct and informative, so this is a
  regression in clarity introduced by the rename rather than a pre-existing flaw.
Reproduction: read engine/src/trace.rs:124 against the body on line 126.
Expected: Say that hits and misses partition every probe, so their sum is the
  probe count — and, worth stating explicitly here, that hash_collisions is
  deliberately excluded because it overlaps hits rather than forming a third
  disjoint category. That overlap is documented on the field at line 19 but is
  exactly the non-obvious fact this accessor's reader needs.

Verification (all run by the reviewer on fe46d6d, not taken from the handoff):
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean,
  re-confirmed with a clean CARGO_TARGET_DIR (exit 0, no warnings), since Cargo
  lint caching can mask a stale pass
- cargo test --workspace: 236 passed, 0 failed, 2 ignored (both pre-existing)
- Target immutability: fe46d6d is an ancestor of tip 5262671; the only files
  changed after it are the task file (handoff and the key-width note). Confirmed
  no implementation file moved after the recorded target.
- Search effect, independently reproduced round-robin over three rounds, base
  9b7bf33 vs target in separate worktrees, release builds, go depth 10 from
  startpos at the default 16MB hash:
    base   4,883,269 nodes, hashfull 294, times 831/950/1244 ms
    target 4,762,311 nodes, hashfull 607, times 883/935/993 ms
  Node counts are bit-identical across all rounds in both directions, and score
  cp 0 with an identical 10-move PV. The 2.5% node reduction, the hashfull
  294 -> 607 density figure, and "time to depth is level within drift" all
  reproduce exactly as BENCHMARKS.md reports them. The machine carried UI load
  (load average ~4.7) so the timings are noisy, but the node count is exact and
  load-independent, and it is the claim that carries the argument.

Assessment (not blocking, recorded so the rework stays bounded):
- I reviewed the snapshot protocol, replacement policy, sizing, telemetry, and
  the search integration against the full base-to-target diff and found no
  defect. The XOR-validated pair genuinely makes a hybrid entry unconsumable, the
  Relaxed justification is correct (validation is value-based, so reordering only
  changes which pairing is observed and every pairing is checked), and both hot
  paths are bounded with no CAS and no retry.
- I specifically chased the one hazard the design introduces: clear_hash now
  panics via Arc::get_mut if a worker still holds the table. I traced every
  caller. SearchHandle::wait() is a real JoinHandle::join(), and the UCI
  ucinewgame path (engine.rs:168-172) and the web UI reset path (game.rs:256-258)
  both stop and join before calling new_game(). A "ucinewgame" during or right
  after a search cannot panic. Worth knowing for later: Drop for SearchHandle
  cancels without joining, so the invariant is upheld by caller discipline rather
  than structurally — no current path drops a live handle, but a future one could.
  That is a pre-existing shape and out of scope here; flagging it, not asking for it.
- On the key-width question raised in comment #2: I agree with the conclusion,
  and the corrected arithmetic is right — 32 bits buys ~25% more entries, not 2x,
  and only after restructuring clusters into separate key and data arrays. Against
  a measured ~1% plausible node gain on a machine whose noise floor is far wider,
  that is not worth a re-implementation. Keep the full key. The Stockfish-style
  10-byte/tolerated-tear tradeoff is a genuine policy question, but it inverts
  AC#2 and AC#13 and correctly belongs to a human and a separate task, not here.
  You were right not to action it and right not to open a follow-up unasked.
- Acceptance criteria: I judge every one of the 16 met on the evidence above.
  None are checked at this verdict only because approval and criterion-checking
  belong to the same act; they will be checked at approval, and I expect no
  further verification work beyond re-running the required checks on the new
  target.
---

author: @codex
created: 2026-07-19 13:45
---
Reviewer-authored fix
Branch: task-57-tt-clustered-snapshots
Previous implementation: fe46d6d81bde8e685f0c69b174805fd629b0c82d
New implementation target: aa55cd18a5eb3f0a5e83b6bc6ec1e3b4e0e37e1f
Resolved findings: REV-1-01, REV-1-02, REV-1-03

Authored by the reviewer under explicit human authorisation, because the three
findings were comment-only and a full rework round was not worth the latency.
This means the usual independence guarantee does not hold for these changes: the
person who raised the findings also fixed them. Approval is therefore deferred to
a fresh review session or to the human owner, and I have not moved the task to
Ready to Merge.

Resolutions:
- REV-1-01: the AC# prefixes are gone from all four test doc comments in tt.rs.
  Nothing else changed; each sentence after the prefix already stood alone.
- REV-1-02: the stride comment now states what the code relies on — the stride is
  at least one, so the visited indices are distinct, and the last sits within a
  stride of the end so the sample spans the allocation. The false power-of-two
  claim is gone.
- REV-1-03: hash_probes says what it sums again, and now records the non-obvious
  part: hash_collisions is excluded because it counts a subset of the hits, so
  adding it would double-count those probes.

Additional change, requested by the human owner as a fly-by on the non-blocking
observation in the previous review comment:

Drop for SearchHandle cancelled the worker but did not join it. Every current
caller waits explicitly, so nothing was broken, but it left the transposition
table's ownership boundary resting on caller discipline: a handle dropped rather
than waited detached a worker that still held a clone of the table, and the next
clear_hash would panic on Arc::get_mut whenever it won the race. Drop now cancels
and joins, which makes "no search is running" structural.

The join always terminates and costs nothing on any existing path:
- cancellation is checked on the search hot path;
- neither channel the worker writes on its way out can block it (events is
  unbounded; the completion channel has capacity for the one message ever sent);
- no current caller drops a live handle, so no existing path reaches the join at
  all. It is a backstop against a future one, not a change to today's control flow.
- the join result is discarded deliberately: there is no consumer for the outcome
  in Drop, and a panicking worker must not panic the dropping thread, which during
  unwinding would abort the process.

This strengthens AC#6 rather than expanding scope: that criterion asks the API to
enforce the ownership boundary that keeps active searches from being invalidated
accidentally, and clear_hash's exclusive reference only delivers that if no worker
can outlive its handle.

Verification on aa55cd1:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean on a
  clean CARGO_TARGET_DIR, no warnings
- cargo test --workspace: 280 passed, 0 failed, 2 ignored (both pre-existing).
  Engine tests went 230 -> 231 with the new test.
- New test has teeth, verified rather than assumed: with Drop reverted to
  cancel-without-join, dropping_a_search_handle_releases_the_table_for_a_later_clear
  fails with "the hash cannot be cleared while a search still holds the table" at
  search.rs:139. Restored, it passes. That also confirms the hazard was reachable
  in practice, not merely theoretical.
- No behavioural change to the transposition table itself; the earlier round-robin
  measurement against base 9b7bf33 still describes this target.
---

author: @codex
created: 2026-07-19 13:53
---
Approval
Reviewed branch: task-57-tt-clustered-snapshots
Approved implementation: aa55cd18a5eb3f0a5e83b6bc6ec1e3b4e0e37e1f
Verdict: approved

Approval authority: the human owner, not an independent agent review. The
reviewer authored the REV-1-01..03 fixes at aa55cd1 under explicit
authorisation, so agent independence does not hold for that commit. The owner
reviewed the resulting diff and signed off. Recording that plainly, because the
lifecycle's usual guarantee is that an agent other than the author approved the
work, and that is not what happened here.

All sixteen acceptance criteria are checked on the evidence below. The base-to-
target diff was reviewed in full, not only the fix.

Verification on aa55cd1, all run by the reviewer:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean on
  a clean CARGO_TARGET_DIR, no warnings. Confirmed this way because Cargo caches
  lint results and the first run finished in 0.82s.
- cargo test --workspace: 280 passed, 0 failed, 2 ignored (both pre-existing)
- Round-robin against base 9b7bf33, release builds in separate worktrees, three
  rounds each, go depth 10 from startpos at the default 16MB hash:
    base   4,883,269 nodes, hashfull 294, times 831/950/1244 ms
    target 4,762,311 nodes, hashfull 607, times 883/935/993 ms
  Node counts bit-identical across every round in both directions, identical
  score and PV. The 2.5% node reduction and the hashfull density figure reproduce
  exactly as BENCHMARKS.md reports them. Timings were taken under UI load and are
  noisy, but the node count is exact and load-independent and carries the claim.
- Drop-join guarantee proven rather than assumed: reverting Drop to
  cancel-without-join makes the new test fail with "the hash cannot be cleared
  while a search still holds the table"; restored, it passes.

Judgement recorded for the merge gate:
- The branch is behind primary. TASK-55 merged as 909d54e after this base, and
  the master comment-standard commits 4025c4b and 74b53d6 also postdate it. None
  of them touch the transposition table, but $merge integrates against the live
  tip and must re-verify there, which is exactly what its gate is for.
- The key-width question raised in comment #2 is settled as "keep the full 64-bit
  key". The Stockfish-style 10-byte entry with tolerated torn reads is a real
  alternative but inverts AC#2 and AC#13, so it belongs to a separate task and a
  human decision, not to this one.
---

author: @codex
created: 2026-07-19 13:56
---
Merge ejected: textual conflict
Primary tip tested: 08e9261b9e44a98c06342806a41178e8259ae9e4
Approved target: aa55cd18a5eb3f0a5e83b6bc6ec1e3b4e0e37e1f
Branch tip merged: c7bcfe2
Result: not merged, primary not advanced

The trial merge was built on a detached HEAD, so nothing was left behind:
primary is still exactly 08e9261 and no implementation file was modified.

Failing command:
  git merge --no-ff c7bcfe2
  Auto-merging engine/src/search.rs
  CONFLICT (content): Merge conflict in engine/src/search.rs
  Automatic merge failed; fix conflicts and then commit the result.

Exactly one conflict hunk, at the quiescence early-cutoff gate in
engine/src/search.rs (around line 1209 of the merged file):

  <<<<<<< HEAD
              // the rule is still out of reach.
              if tt_hit && !entry.is_empty() && self.clock_permits_tt_reuse(entry.depth) {
  =======
              // the rule is still out of reach (TASK-58).
              if let Some(entry) = tt_entry.filter(|e| self.clock_permits_tt_reuse(e.depth())) {
  >>>>>>> c7bcfe2

This is a comment collision, not a semantic one. Master commit 74b53d6 ("docs:
remove process-artifact references from code comments") deleted the "(TASK-58)"
suffix from that comment line. This branch rewrote the immediately adjacent `if`
to the new snapshot API while leaving the comment text as it was at the base
commit, so the two edits land on adjacent lines and git cannot separate them.

The resolution is to take master's comment line together with this branch's code
line — that is, keep "// the rule is still out of reach." above
`if let Some(entry) = tt_entry.filter(...)`. No behaviour is in question and the
two sides do not disagree about any logic.

Note the same class of conflict, against the same master commit, ejected TASK-42
at 1f711d3 ("merge(task-42): eject on comment conflict against 74b53d6"). It was
resolved on the task branch by 96adb9a ("docs(task-42): make time-allocation
comments stand alone"), followed by a corrected implementation target and a fresh
approval. That is the precedent to follow here.

Also worth folding into the same rework: the branch still carries two other
"(TASK-58)" comment references that 74b53d6 removed on master but that did not
conflict, because this branch left them untouched. Merging as-is would silently
reintroduce them. They are in engine/src/search.rs, in the main-search Step 24
store comment and in the quiescence clock-gate comment. Resolving only the
conflicting hunk would leave the branch inconsistent with the standard master now
holds, so all three should be brought into line in one pass.

State of the approval: the sixteen acceptance criteria and the final summary
describe aa55cd1, which was fully verified and remains correct as a description of
that commit. They are left checked deliberately rather than reset, because nothing
about the reviewed work failed — only its integration against a primary branch that
moved. Whatever new target the rework produces supersedes aa55cd1 and needs a fresh
review before another merge attempt.

No benchmarks were run on the merge: it never produced a mergeable commit. For the
record, engine/src/perft.rs contains no transposition-table reference and neither
benches/perft.rs nor benches/movegen.rs imports the table, so those two harnesses
cannot observe this change; the meaningful hot-path evidence is the depth-10 node
count and benches/search.rs, both already recorded against base 9b7bf33.
---

author: @codex
created: 2026-07-19 14:06
---
Conflict resolved, new implementation target
Branch: task-57-tt-clustered-snapshots
Previous target: aa55cd18a5eb3f0a5e83b6bc6ec1e3b4e0e37e1f
New implementation target: 0eec046 (merge of master 08e9261 into the task branch)
Resolved: the merge conflict recorded in the eject comment above

Authorised directly by the human owner, who waived a fresh review on the grounds
that the change is confined to comments. Recording that plainly: this target has
not been independently reviewed, and the comment-only characterisation below is
the evidence for why that was judged acceptable.

What changed since aa55cd1:
- The one conflicting hunk at the quiescence early-cutoff gate now takes master's
  comment line ("// the rule is still out of reach.") above this branch's code
  line (`if let Some(entry) = tt_entry.filter(...)`). Nothing else in the hunk.
- Everything else came from master through the merge, untouched by hand.

On the process-artifact references: no hand-editing was needed beyond that one
line. Master's 74b53d6 had already removed them, and because this branch never
modified those particular comment lines, the merge simply took master's cleaned
version. A sweep of the merged tree for TASK-nnn, AC#n, REV-n-nn and doc-n across
all .rs files outside backlog/ now returns nothing.

Scope of interaction, checked rather than assumed:
- engine/src/search.rs is the only file both master and this branch changed since
  the base commit.
- Master's two commits to it, 13af47e and 74b53d6, are both comment-only, so the
  merge carries no semantic interaction to reason about.
- Master's other work since the base (TASK-42 in engine/src/time.rs, the strength
  tooling, the TASK-63/64 backlog) touches no file this task touches.

Verification on 0eec046:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean on a
  clean CARGO_TARGET_DIR, no warnings
- cargo test --workspace: 284 passed, 0 failed, 2 ignored (both pre-existing). The
  engine suite went 231 -> 235 because master brought four tests with it.
- Old transposition API confirmed absent from the merged search.rs (no tt_hit, no
  Probe::, no into_inner, no write(&self.pos)); the snapshot API and the Drop join
  both confirmed present, so the merge did not partially revert this task's work.

The sixteen acceptance criteria and the final summary continue to describe this
work accurately; the merge changed one comment line and nothing else of substance.
---

author: @codex
created: 2026-07-19 14:10
---
Merged
Primary tip before merge: 08e9261b9e44a98c06342806a41178e8259ae9e4
Merge commit: fec9f5f
Approved target: 0eec046
Result: landed, primary advanced by fast-forward to the verified merge commit

The merge commit was built on a detached HEAD and primary was fast-forwarded to it
only after the integrated checks passed and the tip was re-read and confirmed
unchanged, so primary never pointed at an unverified commit.

Integrated verification on fec9f5f:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean on a
  clean CARGO_TARGET_DIR, exit 0. Run this way deliberately: Cargo caches lint
  results across the trial merges of a retry loop, so a fast pass can reflect a
  previous tip rather than this one.
- cargo test --workspace: 284 passed, 0 failed, 2 ignored (both pre-existing)
- cargo bench --bench perft --bench movegen: generate moves 186.41 ns against the
  BENCHMARKS.md 193.83 ns threshold; perft 5 22.266 ms against the 22.472 ms
  threshold. Both inside the documented limits, and both about 0.4% from the stored
  criterion baseline, which is flat. The roughly 4% absolute gap from the recorded
  figures applies equally to both benchmarks, which is the signature of machine
  conditions rather than a code change: the run was taken under UI load on a machine
  that was not idle. Neither harness can observe this task in any case — engine/src/
  perft.rs contains no transposition-table reference and neither bench imports the
  table. The hot-path evidence that does bear on this change is the depth-10 node
  count, reproduced round-robin earlier: 4,883,269 -> 4,762,311, bit-identical
  across rounds, with identical score and principal variation.

Overlap with recently landed work, checked rather than assumed:
- engine/src/search.rs is the only file this task and post-base master both changed.
  Master's two commits to it, 13af47e (remove a misleading mate-distance pruning
  claim) and 74b53d6 (remove process-artifact references), are both comment-only.
- TASK-42 landed in the same window and touched engine/src/time.rs, which this task
  does not touch. No other overlap exists.

Recorded for the history: approval authority for this task was the human owner, not
an independent agent review. The reviewer authored both the REV-1-01..03 comment
fixes and the merge conflict resolution, and the owner waived a fresh review on the
grounds that those changes were confined to comments. The independence the lifecycle
normally provides was not present here, and the verification evidence above is what
stands in its place.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rewrote the transposition table around cache-line clusters of fully verified, immutable snapshots.

Each 64-byte cluster holds four 16-byte slots, and each slot publishes its entry as `key ^ data` plus `data`. A probe accepts a slot only when the XOR recovers the full 64-bit Zobrist key, which verifies identity outright instead of filtering a truncated signature through move legality, and makes a torn pair unconsumable: a hybrid of two writes can only be accepted on a 64-bit coincidence. `probe` returns an owned `Snapshot` rather than a borrow, so replacement between the probe and the point where the search consumes the result cannot change what it consumes; `store` picks its own victim at store time. Both take `&self` and are bounded and lock-free with no CAS and no retry, so one `Arc<Table>` serves arbitrary Lazy SMP workers with no partitioning or worker-owned state. Replacement distinguishes same-key updates from clashes and weighs depth, exactness and age, so a shallow entry cannot evict a deeper exact one. Clearing is physical and takes `&mut self`, making the new-game ownership boundary a type-system property, and `Drop for SearchHandle` now joins rather than detaches so no worker can outlive its handle and defeat it. Sizing rounds down to a power of two so the allocation never exceeds the advertised limit, and `hashfull` samples on a stride across the whole table, which the previous prefix sample could not do below 1000 entries without panicking.

Verified at aa55cd1: `cargo fmt --check` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean on a clean CARGO_TARGET_DIR; `cargo test --workspace` 280 passed, 0 failed, 2 pre-existing ignored, including deterministic tests for replacement between probe and consumption, a hand-constructed torn pair, a different key sharing a cluster index, age wrap, sizing boundaries, small-table telemetry, and threaded tests asserting no accepted snapshot ever carries invented data. Search effect reproduced independently round-robin against base 9b7bf33 over three rounds at `go depth 10` from the start position: 4,883,269 -> 4,762,311 nodes, bit-identical every round, with identical score and principal variation, and `hashfull` 294 -> 607. Time to depth is level within the machine's drift. `benches/tt.rs` and the BENCHMARKS.md "Transposition table" section record lifecycle, hot-path and multi-worker figures, including the linear clear as a deliberate regression against the old constant-time generation bump.
<!-- SECTION:FINAL_SUMMARY:END -->
