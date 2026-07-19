---
id: TASK-51
title: 'Add late move reduction, extensions, and post-alpha depth reduction'
status: To Do
assignee: []
created_date: '2026-07-18 18:30'
updated_date: '2026-07-19 13:34'
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
