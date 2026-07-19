---
id: TASK-64.17
title: Replace the yielded-flag ordering buffer with partition-and-shrink selection
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-19 13:43'
updated_date: '2026-07-19 15:04'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Order preservation evidence (AC#4). Compared the full UCI info stream between the base commit aec9992 and this branch over 65 positions (start position, Kiwipete, a pawn endgame, a promotion race, a capture-promotion position, and the first 60 WAC positions), searching each to depth 10 with a fresh engine process. The records compare depth, score, node count, hashfull and principal variation at every iteration, with the wall-clock fields (nps, time) stripped. The two records are byte-identical across all 873 lines, covering 312,863,482 nodes at the final iteration. Identical principal variations as well as identical node counts is a stronger check than node counts alone: a reordering that happened to preserve a count would still surface as a different line.

Underpromotion ordering hazard found and covered. Selection now reorders the buffer in place, and underpromotions are derived from the queen promotion segment, so deriving them after the promotion phase had sorted would make their order depend on that sort. The hash-move duplicate makes this observable: the queen promotion matching the hash move is not yielded again, but its rook, knight and bishop siblings are ordinary moves that must still be searched in the same relative position. Underpromotions are therefore expanded at promotion-load time, while the segment is still in generation order. The regression test for this was confirmed to fail when the expansion is moved after the partition, so it is not vacuous.

Search benchmark (AC#5). Measured round-robin between a detached worktree at the base commit aec9992 and this branch, alternating base and target within each round, six paired rounds in total, taking the minimum per configuration as BENCHMARKS.md prescribes.

| Configuration | Base aec9992 | This branch | Change |
| --- | ---: | ---: | ---: |
| search startpos depth 7 | 41.306 us | 38.762 us | -6.2% |
| search startpos depth 7 no deadline | 40.326 us | 37.841 us | -6.2% |

The target was faster in five of six paired rounds in each configuration. The one exception was a round in which both builds measured with confidence intervals three times their usual width, from competing load.

Machine conditions, stated plainly. This was not an idle machine and I could not make it one: other worktrees in this repository had active sessions running test binaries and rustc compiles throughout, at times consuming three cores. Both columns are inflated relative to the documented baseline of 40.25 us and 39.73 us for that reason. Alternating base and target within each round and taking minimums is the discipline BENCHMARKS.md defines for exactly this drift, and the relative figure is consistent across rounds. The absolute figures are not trustworthy enough to become a new baseline, so BENCHMARKS.md is deliberately left unchanged; adopting these numbers as a baseline should wait for a genuinely idle machine.

Where the gain comes from, and whether it lasts. History updates are currently commented out in search.rs, so every quiet move scores zero. Selection over the quiet segment therefore always finds its maximum at the first remaining entry and performs no rotations at all, and the measured gain is purely the halved scan: draining n quiets costs about n^2/2 comparisons instead of n^2. Once history scoring is live, rotations start to cost something on the quiet segment and this figure should be re-measured rather than assumed to carry over. The halved scan and the capture partition both remain wins regardless.

Smaller items folded in, as the task listed them.

Score range constraint (AC#6). Selection seeded its running maximum with i16::MIN and compared with strict greater-than, so an entry scored exactly i16::MIN could never be yielded. The shrinking selection seeds from the first remaining entry instead, which removes the constraint rather than documenting it. Covered by a test.

PhaseIter (AC, folded). The pass-through match mapping Some to Some and None to None is gone; PhaseIter now implements Iterator directly over the inner enum.

Segment setters and accessors (folded). The six set_*_segment methods collapse into one close_segment helper, and the six accessors into safe slice indexing. The ranges live in a Segments struct with named fields rather than an array indexed by Phase, because the promotion and underpromotion phases each own two adjacent subranges and so do not fit one range per phase. This is stated in a comment on the struct.

Documented size (AC#7). The doc claimed 3KB. Removing the flag shrinks the entry from 8 bytes to 6, and the extra ranges add 64 bytes, for a measured 1704 bytes. The doc now says so and a test asserts it, so the comment cannot silently drift again.

Unsafe segment construction (AC#8). Replaced with safe indexing rather than re-justified. The check being avoided was one per segment construction, not one per move, and the benchmark above shows the search got faster with the bounds checks in place, so the recorded measurement does not support keeping the unsafe.

Scope note. The capture partition uses rotations, which is O(C^2) element moves in the worst case rather than the O(C) a scratch-buffer partition would give. Captures number in the tens at most and each move is 6 bytes, and the alternatives are a scratch buffer the size of the segment or O(n log n) rotations. The reasoning is recorded in the function's doc comment.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 15:04
---
Implementation handoff
Branch: task-64.17-partition-shrink-ordering
Worktree: /Users/seabo/seaborg-worktrees/task-64.17-partition-shrink-ordering
Base: aec999283d9f4c623c27a2badfb95c3cd7737a59
Implementation target: b2790cba9b03c5fcac76521f1357ca3bc173c6ef
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 291 tests across all targets, 0 failures, 2 pre-existing ignored
- UCI node-count equivalence vs base aec9992, 65 positions to depth 10: byte-identical info streams, 873 lines, 312,863,482 nodes at the final iteration
- cargo bench --bench search, six paired round-robin rounds vs base: -6.2% in both configurations
Known failures: none

Reviewer notes. Two things are worth attention. First, the order-preservation argument rests on selection taking the first maximum and rotating rather than swapping it into place; a swap looks equivalent and is not, and the test selection_yields_by_score_then_generation_order is what pins this. Second, underpromotions are now expanded eagerly at promotion-load time rather than at the underpromotion phase, because in-place selection reorders the promotion segment; underpromotions_survive_a_queen_promotion_that_duplicates_the_hash_move covers it, and I confirmed that test fails when the expansion is moved back after the partition, so it is not vacuous.

The benchmark was taken on a machine carrying competing load from other worktrees in this repository, which I could not quiet. The relative figure is consistent across rounds but the absolute numbers are inflated, so BENCHMARKS.md is deliberately left unchanged.
---
<!-- COMMENTS:END -->
