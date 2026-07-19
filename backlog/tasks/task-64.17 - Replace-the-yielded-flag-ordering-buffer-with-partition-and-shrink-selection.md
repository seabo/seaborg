---
id: TASK-64.17
title: Replace the yielded-flag ordering buffer with partition-and-shrink selection
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 13:43'
updated_date: '2026-07-19 14:25'
labels:
  - search
  - move-ordering
  - architecture
  - refactor
  - performance
dependencies: []
references:
  - engine/src/ordering.rs
  - engine/src/search.rs
  - core/src/movelist.rs
parent_task_id: TASK-64
priority: high
type: enhancement
ordinal: 66500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Move ordering yields each move by scanning an entire segment and marking the winner with an interior-mutability flag. Replace that representation with a partition-and-shrink selection, which removes the flag, stops rescanning entries already consumed, and makes the iterator honest about the fact that it consumes.

Current representation. Each buffer slot is an Entry holding a scored move and a `Cell<bool>` named `yielded` (ordering.rs:16-19). `SelectionSort::next` loops the whole segment, skips flagged entries, tracks a running maximum and flags the winner (ordering.rs:99-124). Cost is O(n) per yielded move over the full segment including entries already consumed, so draining a segment of n moves costs about n^2 comparisons rather than the n^2/2 of a conventional shrinking selection sort.

Three phases share one capture segment. `good_capt_iter`, `equal_capt_iter` and `bad_capt_iter` (ordering.rs:580-624) each build a SelectionSort over the same `capt_segment` under a different predicate, so each phase rescans every capture the earlier phases already yielded. One three-way partition of the capture segment into good, equal and bad subranges immediately after scoring is O(C) and lets each phase sort only its own subrange.

The quiet segment is where the cost concentrates. It is the largest segment at most nodes, and until TASK-64.2 lands every quiet scores zero, so the quadratic scan currently buys generation order at full price.

The flag also creates an API hazard. Because `yielded` is a Cell, IntoIterator is implemented for `&OrderedMoves` (ordering.rs:680-700) while iteration mutates. The type advertises a shared borrow but consumes, so iterating one phase twice silently yields nothing the second time. The module doc acknowledges this as confusing (ordering.rs:214-216). Removing the flag lets `next` take `&mut self` and the hazard goes with it.

Behaviour preservation. A shrinking selection that keeps the first maximum on ties yields exactly the order the current implementation yields. This change should therefore be verified by identical node counts at fixed depth, not by a strength run.

Smaller items in the same code, to fold in rather than track separately:
- SelectionSort seeds its running maximum with `i16::MIN` and compares with strict greater-than, so an entry scored exactly `i16::MIN` can never be yielded (ordering.rs:106-115). Unreachable from static exchange evaluation today, but it is an undocumented constraint on what a Loader may assign.
- PhaseIter wraps IterInner and forwards `next` through a match mapping Some to Some and None to None (ordering.rs:668-678).
- Six near-identical `set_*_segment` methods and six accessors (ordering.rs:343-446) express what one array indexed by phase expresses directly.
- The struct doc claims 3KB; measured size is 2152 bytes (ordering.rs:220-223).
- `segment_from_range` and `segment_from_range_mut` use raw pointers to skip slice bounds checks, justified as measurable (ordering.rs:460-485). The check avoided is one per segment construction, not one per move. Re-measure and prefer safe indexing unless the measurement holds up.

Sequencing. TASK-64.10 adds a phase variant and TASK-64.11 changes capture scoring. Both are cheaper to write against the replacement representation than against the flag scheme, which is why they depend on this task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Moves are selected without a per-entry yielded flag, and no entry already yielded is rescanned when selecting the next move
- [ ] #2 The capture segment is partitioned once into good, equal and bad subranges, and each capture phase sorts only its own subrange
- [ ] #3 The phase iterator borrows mutably, so a phase cannot be silently iterated twice through a shared reference
- [ ] #4 Node counts at fixed depth are identical to the pre-change commit on a representative position set, confirming the change is order-preserving
- [ ] #5 The search benchmark is recorded before and after, on an idle machine per BENCHMARKS.md discipline
- [ ] #6 Any constraint on the score range a Loader may assign is either removed or documented and asserted
- [ ] #7 The OrderedMoves doc comment matches the implementation, including its actual size
- [ ] #8 The unsafe in segment construction is either justified by a recorded measurement or replaced with safe indexing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace the buffer element with a bare ScoredMove, deleting the Cell<bool> yielded flag.
2. Make deduplication physical rather than flag-based. The deduplicated segment is always the last segment in the buffer, so matching moves are removed by stable left-compaction and the buffer is truncated. Add ArrayVec::truncate in core to support this. Fold the per-yield hash/killer checks in KillerIter and QuietsIter into this one load-time pass, which collapses both iterators.
3. Add a stable in-place partition helper over a segment. Use it once on the capture segment to produce good/equal/bad subranges, and once each on the promotion and underpromotion segments to put capture-promotions first, replacing the predicate-filtered rescans.
4. Rewrite SelectionSort as a shrinking selection over &mut [ScoredMove]: scan only segment[head..], take the first maximum, rotate it into position head so the relative order of the unyielded remainder is preserved, then advance head. Rotation rather than swap is what makes the yielded order identical to the flag scheme on ties. Seed the running maximum from the first remaining candidate instead of i16::MIN, so every i16 score is selectable and the undocumented Loader constraint disappears.
5. Yield Move by value. A shrinking selection mutates the segment as it goes, so it cannot hand out references borrowed for the whole iteration; Move is Copy and 4 bytes.
6. Implement IntoIterator for &mut OrderedMoves and collapse PhaseIter's pass-through match.
7. Replace the six set_*_segment methods with one close_segment helper, and the six accessors with safe slice indexing. Remove the unsafe pointer segment construction and record the benchmark that justifies the replacement.
8. Update the OrderedMoves doc comment to the measured size, and add a test asserting that size so the doc cannot silently drift.
9. Verify order preservation by comparing fixed-depth UCI node counts against the base commit over a position set, and run the search benchmark round-robin against a base worktree per BENCHMARKS.md.
<!-- SECTION:PLAN:END -->
