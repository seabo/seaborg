---
id: TASK-52
title: Reduce search depth on transposition-table misses
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-18 18:45'
updated_date: '2026-07-25 18:30'
labels: []
dependencies:
  - TASK-51
references:
  - engine/src/search.rs
priority: medium
type: feature
ordinal: 52000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search steps 11 and 13 are unimplemented placeholders. Both express the same idea - when no transposition-table move is available, the node is likely cheap to get wrong, so search it shallower - and differ only in node type and margin:

Step 11 (search.rs:604): in PV nodes, if the move is not in the TT, decrease depth by 3.
Step 13 (search.rs:610): in non-PV nodes with depth >= 7 and not in the TT, decrease depth by 2.

They are naturally paired and share one measurement. This depends on nothing in TASK-50 or TASK-51 mechanically; the dependency is purely sequencing, so that strength results remain attributable to one change at a time.

Note that TASK-12 repaired TT reuse and mate-score semantics, so TT hit and miss are now trustworthy signals to branch on.

The numbered Step N comments in search.rs are a deliberate map of the intended search structure. Replace the TODO markers with implementations; do not delete the step comments.

TODO sites: engine/src/search.rs:604, :610.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PV nodes with no transposition-table move are searched at reduced depth per step 11
- [ ] #2 Non-PV nodes at depth >= 7 with no transposition-table move are searched at reduced depth per step 13
- [ ] #3 Reduction is driven by a genuine TT miss and not by a collision-guard rejection, consistent with the semantics established by TASK-12
- [ ] #4 Measured with the TASK-27 strength-regression script showing no strength loss, with results recorded in the implementation notes
- [ ] #5 The step 11 and step 13 TODO markers are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make search_inner's depth parameter mutable.
2. Add IIR constants (IIR_PV_REDUCTION=3, IIR_NON_PV_REDUCTION=2, IIR_NON_PV_MIN_DEPTH=7) near the other search constants.
3. In Step 3's TT-probe match, track a tt_collision flag set only in the collision-guard branch, so IIR can distinguish a genuine miss/moveless entry from a Zobrist-collision rejection (AC#3, TASK-12 semantics).
4. Replace Step 11 TODO: PV node with no genuine TT move -> depth = (depth - 3).max(1). Floor at 1 because Step 5's quiescence handover is already behind us.
5. Replace Step 13 TODO: non-PV node, depth>=7, no genuine TT move -> depth -= 2.
6. Add iir_disabled test hook field + iir_enabled() gate, matching the rfp/lmr toggle convention.
7. Retain all numbered step comments; write reader-facing rationale.
8. Tests: IIR reduces the tree when it fires (PV and non-PV); IIR does not fire when a TT move is present; genuine-miss vs collision semantics covered by structure. Run fmt/clippy/test.
9. AC#4: run the TASK-27 strength-regression script; record results in notes.
<!-- SECTION:PLAN:END -->
