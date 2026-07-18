---
id: TASK-46
title: Prevent aborted search subtrees from contributing scores
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 18:29'
updated_date: '2026-07-18 21:54'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Introduce an explicit aborted node-search outcome and propagate it through main search, razoring, quiescence, and check-evasion recursion while always unmaking moves before unwinding.
2. Make iterative deepening commit scores and PV state only after a fully completed iteration, restoring the prior completed PV on abort.
3. Add deterministic regression coverage for a mid-subtree abort, last-completed bestmove/PV preservation, and absence of TT writes from the aborted node.
4. Run focused tests and all repository-required formatting, strict Clippy, and workspace tests; commit implementation and record the immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented explicit `Option<Score>` node outcomes across main search, razoring, quiescence, and check-evasion recursion. Aborted children unwind only after restoring the position and cannot update alpha, best move, PV, or ancestor TT entries. Iterative deepening now restores the prior completed PV when a candidate iteration aborts. Added a deterministic node-threshold regression that aborts within the depth-two subtree and verifies the depth-one result/PV/root TT entry remain authoritative.
<!-- SECTION:NOTES:END -->
