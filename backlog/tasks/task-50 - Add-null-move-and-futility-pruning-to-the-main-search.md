---
id: TASK-50
title: Add null move and futility pruning to the main search
status: To Do
assignee: []
created_date: '2026-07-18 18:30'
labels: []
dependencies:
  - TASK-46
references:
  - engine/src/search.rs
priority: medium
type: feature
ordinal: 50000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search steps 8 and 9 are unimplemented placeholders. Both are forward-pruning techniques that share the same guard conditions (not in check, non-PV node, a usable static evaluation) and the same validation burden, so they are delivered and measured together.

Step 8, futility pruning: near the horizon, skip quiet moves whose static evaluation plus a margin cannot reach alpha.
Step 9, null move search with verification: give the opponent a free move at reduced depth; if the result still fails high, prune. Verification search is required to avoid zugzwang blunders.

The numbered Step N comments in search.rs are a deliberate map of the intended search structure. Replace the TODO markers with implementations; do not delete the step comments.

Sequencing: this is gated on TASK-46 because pruning decisions that read alpha and the transposition table compound the aborted-subtree score problem rather than tolerate it. This ticket and TASK-51 and TASK-52 must land one at a time, since concurrent search changes make strength-regression attribution impossible.

Caveat to check first: these prunings assume the static evaluation is informative enough for pruning decisions to be trustworthy. Assess the current evaluation before committing to the approach, and report back if it is too coarse (for example material-only) for a gain to be expected.

TODO sites: engine/src/search.rs:595 (futility), engine/src/search.rs:598 (null move).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Futility pruning is implemented at step 8 and is disabled in PV nodes and when in check
- [ ] #2 Null move pruning with a verification search is implemented at step 9 and is disabled in PV nodes, when in check, and in likely-zugzwang positions
- [ ] #3 Both techniques are measured with the TASK-27 strength-regression script and show no strength loss, with results recorded in the implementation notes
- [ ] #4 A fixed-depth search on a known position set returns the same best moves as before where pruning is inactive, confirming the guards
- [ ] #5 The evaluation-quality assessment is recorded, including the decision to proceed or to defer
- [ ] #6 The step 8 and step 9 TODO markers are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->
