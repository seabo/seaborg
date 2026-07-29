---
id: TASK-93
title: Static exchange evaluation reveals x-rays from the wrong square
status: To Do
assignee: []
created_date: '2026-07-29 18:38'
labels:
  - search
  - ordering
dependencies: []
references:
  - engine/src/see.rs
priority: high
type: bug
ordinal: 162000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SEE mis-values many exchanges because the x-ray reveal step queries the vacated origin square instead of the from->to ray, injecting sliders that cannot legally recapture on the target.

When an attacker leaves `from`, `see` (engine/src/see.rs:78) does:

    atta_def |= self.pos.attack_defend_sliding(occ, from) & !processed;

`attack_defend_sliding` (chess/src/position/mod.rs:1100) returns every sliding piece bearing on the square passed, in ANY direction. Only sliders collinear with the from->to ray actually bear on `to`. A slider that attacks `from` along a different line (an enemy rook on `from`s rank, a bishop on the opposite diagonal) is falsely OR-ed into the attacker/defender set of `to` and can then be picked by `least_valuable_piece` as a phantom recapturer. This over-counts enemy defenders and can flip the sign of the exchange.

Reproduced against the current code (added as scratch tests, both fail):
- `k7/8/8/3p4/r3P3/8/8/7K w - - 0 1`, e4xd5 (free pawn, true SEE +100) returns Cp(0): the black rook on a4 attacks e4 along rank 4 and is treated as a defender of d5.
- `k7/8/8/8/3p4/8/8/r2R3K w - - 0 1`, Rd1xd4 (free pawn, true SEE +100) returns Cp(-400): the black rook on a1 attacks d1 along rank 1 and is treated as recapturing on d4.

The sign-flip case is the damaging one: a free winning capture computed as SEE<0 is (a) pruned outright in quiescence (see<0 vs QUIESCENCE_SEE_THRESHOLD=0, engine/src/search.rs ~3872) so it is never searched, and (b) mis-ordered good->bad in main-search capture ordering (engine/src/search.rs ~4341). Because the x-ray candidate set includes pawns, this also fires on ordinary pawn captures, and the triggering geometry (a heavy piece capturing up a file/diagonal with an enemy slider sharing the origins rank/file/diagonal; pawn captures with an enemy rook on the pawns rank) is common in real middlegames.

The conventional fix is to recompute sliders that bear on `to` after updating occupancy, e.g. `atta_def |= self.pos.attack_defend_sliding(occ, to) & !processed & occ`, which cannot introduce a slider that does not actually reach `to`; alternatively restrict the revealed set to the line through `from` and `to`. Related prior work in the same function: TASK-49 (SEE promotions).

Found via an automated correctness audit; it is the highest-impact defect that audit surfaced. This is a behavioural change touching the qsearch prune gate and capture ordering across the whole search, so it must be measured, not asserted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When an attacker vacates `from`, SEE only reveals sliders that actually bear on `to`; no slider that attacks `from` off the from->to ray is counted as an attacker or defender of `to`
- [ ] #2 Regression tests assert `k7/8/8/3p4/r3P3/8/8/7K w - - 0 1` e4xd5 returns +100 and `k7/8/8/8/3p4/8/8/r2R3K w - - 0 1` Rd1xd4 returns +100 (both currently wrong)
- [ ] #3 The existing SEE suite (engine/src/see.rs tests) still passes
- [ ] #4 Change measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
