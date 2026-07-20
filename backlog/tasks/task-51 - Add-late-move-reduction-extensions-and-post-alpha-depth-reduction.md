---
id: TASK-51
title: 'Add late move reduction, extensions, and post-alpha depth reduction'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-20 22:55'
labels: []
dependencies:
  - TASK-50
  - TASK-64.1
  - TASK-64.2
references:
  - engine/src/search.rs
priority: medium
type: feature
ordinal: 51000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search steps 16 and 17 are unimplemented placeholders, and a third TODO inside the move loop records the same idea from another angle. All three are one mechanism and cannot be split without shipping half a feature.

Step 16, reductions and extensions: extend promising lines (for example check evasions) and reduce unpromising ones before searching.
Step 17, late move reduction: search moves late in the ordering at reduced depth, re-searching at full depth only when the reduced search beats alpha.
engine/src/search.rs:692: after a move raises alpha, reduce the depth used for the remaining moves. This is LMR bookkeeping and belongs with the above.

LMR quality depends directly on move-ordering quality, which depends on SEE. TASK-49 corrects SEE for promotions; it is not a hard blocker here, but if TASK-49 is still open when this is picked up, note the interaction when interpreting strength results.

The numbered Step N comments in search.rs are a deliberate map of the intended search structure. Replace the TODO markers with implementations; do not delete the step comments.

Sequencing: gated on TASK-50 so that strength changes can be attributed to one technique at a time.

TODO sites: engine/src/search.rs:637, :640, :692.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Late move reduction is implemented with a re-search at full depth whenever the reduced search raises alpha
- [ ] #2 Reductions and extensions are implemented at step 16 and are not applied in PV nodes where they would truncate the principal variation
- [ ] #3 Remaining moves after an alpha raise are searched at the reduced depth described at search.rs:692
- [ ] #4 The reported principal variation remains legal and complete under reduction, verified against the regression coverage added by TASK-36
- [ ] #5 Measured with the TASK-27 strength-regression script showing no strength loss, with results recorded in the implementation notes
- [ ] #6 The step 16 and step 17 TODO markers and the search.rs:692 TODO are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Compute a per-node in-check flag once before the move loop; reuse for the extension.
2. Step 16 (extensions): check-evasion extension of +1 ply when the side to move is in check; new_depth = depth - 1 + extension. All child searches in the loop use new_depth.
3. Step 17 (LMR): compute a reduction r for quiet, non-extended, late moves at depth >= 3, applied to the zero-window scout at Step 19. Re-search at full new_depth whenever the reduced scout raises alpha (AC#1). Gate under a test-only lmr_disabled hook mirroring forward_pruning.
4. Post-alpha (former :692 TODO): once a move raises alpha in a PV node, treat remaining quiet moves as reducible even before the late-move threshold and take an extra ply off (AC#3). Replace the bare TODO with the mechanism comment; effect realized at Step 17 via did_raise_alpha.
5. Reductions never truncate the PV: move 1 is never reduced, and any reduced scout that beats alpha is re-searched at full depth before a PV re-search populates the PV table (AC#2).
6. Retain the numbered Step 16/17 comments; replace the three TODO markers with implementations (AC#6).
7. Tests: LMR does not change sound (forced-mate/clean-win) results vs lmr-disabled; LMR reduces the tree; existing TASK-36 PV-legality tests still pass (AC#4). Run fmt, clippy, cargo test. Attempt TASK-27 strength smoke and record (AC#5).
<!-- SECTION:PLAN:END -->
