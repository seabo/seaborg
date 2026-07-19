---
id: TASK-60
title: Complete transposition-table integration across main and quiescence search
status: To Do
assignee: []
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 00:25'
labels:
  - transposition-table
  - search
  - quiescence
  - correctness
  - performance
dependencies:
  - TASK-46
  - TASK-57
references:
  - engine/src/search.rs
  - engine/src/tt.rs
priority: high
type: enhancement
ordinal: 59000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Integrate the rewritten transposition table from TASK-57 consistently across main search and quiescence. Search currently couples main-search score reuse to the presence of a valid stored move, so terminal or move-less entries cannot cut off, while quiescence consumes deeper entries but never stores its own results. Quiescence also independently applies the fifty-move boundary at 50 plies instead of the shared 100-ply rule. Establish explicit search-level semantics for score hits, move ordering, depth, bounds, terminal nodes, collisions, and incomplete work. TASK-28 records the existing collision-verification asymmetry and should be resolved by this task rather than duplicated.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A verified score hit and a usable hash move are represented independently, so valid terminal or move-less entries can cut off without being treated as ordering moves
- [ ] #2 Quiescence stores reusable exact and bound results with depth semantics that cannot be confused with insufficiently searched main or quiescence entries
- [ ] #3 Main and quiescence search use the shared 100-ply fifty-move predicate, with regression coverage from halfmoves 50 through 100
- [ ] #4 Collision verification behavior is consistent and documented across both searches, resolving the decision recorded in TASK-28
- [ ] #5 No stopped or incomplete subtree can publish a TT entry, preserving the guarantees of TASK-46
- [ ] #6 Warm-table tactical and terminal-position tests demonstrate correct scores and reduced or equal node counts versus a cold table
<!-- AC:END -->
