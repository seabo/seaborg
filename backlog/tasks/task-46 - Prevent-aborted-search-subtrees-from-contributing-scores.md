---
id: TASK-46
title: Prevent aborted search subtrees from contributing scores
status: To Do
assignee: []
created_date: '2026-07-18 18:29'
labels: []
dependencies: []
references:
  - engine/src/search.rs
priority: high
type: bug
ordinal: 46000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When Search::stopping() is true, the search returns Score::zero() and unwinds. That zero is indistinguishable from a real draw score, so an aborted subtree can raise alpha, become best_move, or be written to the transposition table as a genuine evaluation. The engine then acts on a value that was never searched.

This is the same failure family as TASK-32 (illegal null move at fast time controls) and TASK-34 (self-play instability). Those were fixed at the reporting boundary; this is the underlying score-propagation path.

Audit every early return guarded by stopping() in the main search and quiescence search, and make aborted results unusable rather than plausible: the caller must be able to tell that a subtree was abandoned and must discard it instead of folding it into alpha, best_move, the PV, or the TT.

TODO site: engine/src/search.rs:815 (is this robust?).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An abort during a subtree search cannot cause that subtree value to raise alpha or be recorded as best_move
- [ ] #2 An abort during a subtree search cannot write an entry to the transposition table
- [ ] #3 An abort cannot corrupt the PV reported for the last completed iteration
- [ ] #4 A regression test drives a search to abort mid-subtree and asserts the returned bestmove matches the last fully completed iteration
- [ ] #5 The is this robust? TODO at engine/src/search.rs:815 is resolved and removed
<!-- AC:END -->
