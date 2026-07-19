---
id: TASK-60
title: Complete transposition-table integration across main and quiescence search
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 14:25'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Separate score reuse from move ordering in search_inner Steps 3-4: decode a usable ordering move independently of the score-hit decision, and gate the cutoff on the verified snapshot alone so terminal and move-less entries can cut off.
2. Establish explicit TT depth semantics: a named quiescence draft (0) that main-search stores can never produce, so a quiescence entry can never satisfy a main-search depth requirement.
3. Make quiescence a writer: store stand-pat cutoffs, move-loop cutoffs, terminal mates and fall-through values at the quiescence draft, with bounds classified against the node's own window.
4. Make the quiescence TT block cutoff-only (no window narrowing), so a stored bound is always classified against a window this node owns.
5. Suppress quiescence stores after an abort or a history-sensitive draw claim, matching the main search's guarantees.
6. Document collision-verification semantics at both probe sites: full-key verification is the identity check; valid_move only decides whether a stored move is usable for ordering here. Resolves the TASK-28 decision.
7. Tests: fifty-move agreement across halfmoves 50..=100 for both searches; move-less terminal cutoff; quiescence store round-trip and abort/draw suppression; warm-vs-cold score equality and node-count non-increase.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Score reuse and move ordering are now independent in the main search (Step 3/4). A verified snapshot cuts off on its own merits, so terminal and fail-low entries — move-less by construction — became usable. `valid_move` no longer gates the score; it only decides whether the stored move is a usable ordering hint, and its failure counts a genuine Zobrist collision.

Collision verification is now identical in both searches: the full 64-bit key check inside `Table::probe` is the identity proof, and neither search requires a playable move before trusting a score. Quiescence does not consult the stored move at all because `QMoveLoader` has no hash phase. Documented at both cutoff sites. This is the TASK-28 decision: align, do not keep divergent — TASK-28 is superseded by this work and needs no separate implementation.

Quiescence now stores at a reserved draft (`Search::QUIESCENCE_DRAFT` = 0). The main search cannot write it, because a depth-zero node delegates to quiescence before reaching its own store, so the ordinary `entry.depth() >= depth` test separates the two entry kinds with no extra field: a main-search node needs depth >= 1, which no quiescence entry satisfies.

Quiescence's TT block became cutoff-only. Narrowing the live window from a stored bound would make the node's own store classify its result against a window a previous search supplied; cutoff-only keeps every recorded bound referenced to this node's window.

Stand-pat cutoffs store the fail-soft `stand_pat` rather than the returned `beta`. Both are valid lower bounds; the former is stronger and lets a later visit with a higher beta still cut off.

Abort and history-draw suppression now hold for quiescence as they already did for the main search: an aborted subtree propagates `None` before any store, and `store_quiescence` carries the same `history_draws` comparison as Step 24.

Replacement trade worth noting for review: quiescence writes are far more numerous than main-search writes and always evict the weakest slot in their cluster. The depth-weighted policy makes draft-0 entries the most replaceable, and the warm-vs-cold node-count test shows no regression on the positions covered, but replacement tuning for the new write volume was not attempted here.

Two new tests initially failed on their own construction, not on the implementation, and were corrected: the fifty-move sweep needed a pawn so the main search has a clock-resetting escape below 100 (otherwise a depth-one search legitimately scores zero from clock 99), and the quiescence history-draw test needed clock 99 rather than 98 so a single quiet evasion reaches the boundary.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 14:25
---
Implementation handoff
Branch: task-60-tt-integration
Worktree: /Users/seabo/seaborg-worktrees/task-60-tt-integration
Base: aec999283d9f4c623c27a2badfb95c3cd7737a59
Implementation target: c063b0b
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 291 passed / 0 failed / 2 ignored
- cargo test --release -p engine wac_root_scores_format_without_panicking -- --ignored: pass (run explicitly because this change alters cutoff behavior, which that sweep guards)
Known failures: none
---
<!-- COMMENTS:END -->
