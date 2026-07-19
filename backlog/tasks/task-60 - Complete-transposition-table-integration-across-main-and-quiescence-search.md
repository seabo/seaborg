---
id: TASK-60
title: Complete transposition-table integration across main and quiescence search
status: Done
assignee:
  - '@codex'
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 15:03'
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
- [x] #1 A verified score hit and a usable hash move are represented independently, so valid terminal or move-less entries can cut off without being treated as ordering moves
- [x] #2 Quiescence stores reusable exact and bound results with depth semantics that cannot be confused with insufficiently searched main or quiescence entries
- [x] #3 Main and quiescence search use the shared 100-ply fifty-move predicate, with regression coverage from halfmoves 50 through 100
- [x] #4 Collision verification behavior is consistent and documented across both searches, resolving the decision recorded in TASK-28
- [x] #5 No stopped or incomplete subtree can publish a TT entry, preserving the guarantees of TASK-46
- [x] #6 Warm-table tactical and terminal-position tests demonstrate correct scores and reduced or equal node counts versus a cold table
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

author: @codex
created: 2026-07-19 14:53
---
Review verdict: APPROVED

Implementation target: c063b0b (immutable; branch tip 9583de5 changes only the task file)
Base: aec9992
Reviewed: full diff aec9992..c063b0b, 2 files (engine/src/search.rs, task file). No unrelated changes.

Required checks re-run by the reviewer on c063b0b:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings, confirmed with a clean CARGO_TARGET_DIR so the result is not a cached lint
- cargo test --workspace: 291 passed / 0 failed / 2 ignored
- No new #[allow] introduced by the diff.

Acceptance criteria evidence:
- #1 a_verified_entry_without_a_move_still_cuts_off_the_main_search and an_unplayable_stored_move_costs_ordering_but_not_the_score prove score reuse and ordering are independent, and that an unplayable move still counts one collision.
- #2 quiescence_publishes_its_result_at_the_reserved_draft and a_quiescence_entry_cannot_satisfy_a_main_search_depth_requirement prove the reserved draft separates the two entry kinds through the ordinary depth test.
- #3 both_searches_claim_the_fifty_move_draw_at_the_same_hundred_ply_boundary sweeps every clock 50..=100 for the predicate, quiescence and the main search.
- #4 Both probe sites document that Table::probe's full-key check is the identity proof and that valid_move only decides ordering usability; behaviour is now identical in both searches.
- #5 an_aborted_quiescence_subtree_publishes_nothing and a_history_sensitive_quiescence_value_is_not_stored, plus code inspection of every quiescence exit: each store is dominated by a completed move loop, and child? propagates an abort before any store.
- #6 a_warm_table_matches_the_cold_result_and_never_costs_more_nodes. Independently reproduced on eight further positions (middlegame, endgame, Kiwipete, Lasker-Reichhelm) at depths 5-10: score and best move identical, warm nodes strictly lower on every one, and the root entry survived at full depth in each.

Soundness review beyond the criteria:
- The Step 4 window-narrowing branches in the main search are unreachable in effect: Step 4 runs only for NonPv nodes, which the entry assertion guarantees are zero-window, so any narrowing immediately collapses alpha >= beta and returns. The alpha == beta to alpha >= beta change is therefore behaviour-preserving and correct. The same reasoning makes quiescence's removal of its narrowing branches a behavioural no-op, so cutoff-only costs nothing.
- quiescence_bound's Exact classification is sound: QMoveLoader uses SEE for ordering only and prunes nothing, so a raised alpha is a true quiescence value, and a raise strictly inside the window is guaranteed because a value reaching beta cuts off first.
- Storing the fail-soft stand_pat rather than the returned beta is a valid and strictly stronger lower bound.
- Comment quality: no comment cites a task ID, acceptance criterion, finding ID or Backlog document; 'Rule 1' resolves to the numbered list in the same doc comment; the reasons are stated rather than referenced.

Performance: the search benchmark was run base-vs-target interleaved over six rounds on the same machine. The machine was NOT idle (a competing release binary from another worktree held a core throughout), so absolute figures are not comparable to BENCHMARKS.md; comparing the minimum of each series, the target is +0.2% on 'search startpos depth 7' and +1.1% on the no-deadline variant, both far inside the 5% investigate threshold and comparable to the run-to-run spread. perft and movegen were not run: the diff touches no move generation or shared code, so it cannot affect them.

Non-blocking observation recorded for the record, not a change request:
Table::store's same-key path keeps the existing entry only when existing.depth > depth AND same age AND NOT (new is Exact while existing is not). A quiescence Exact store at draft 0 therefore evicts a deeper non-Exact main-search entry for the same key. Confirmed directly: seeding depth 8 / Bound::Lower then storing depth 0 / Bound::Exact leaves a depth-0 entry. The implementation notes' claim that 'the depth-weighted policy makes draft-0 entries the most replaceable' holds for victim selection across a cluster but not for this same-key path. Measured impact is negligible, which is why this does not block: quiescence only produces Exact at PV quiescence nodes, and instrumenting a real search over eight positions counted 0-1 such evictions per search, with no warm-vs-cold node regression and no benchmark regression. The notes already flag replacement tuning for the new write volume as unattempted; this is the specific mechanism a future tuning pass should address.

Also for the human merging this: TASK-28 is still 'To Do' on the board. Acceptance criterion #4 and the implementation notes treat it as superseded and resolved by this work, which the code supports, but its status needs closing separately. A reviewer may not create or retire tasks, so it is raised here rather than actioned.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Separated TT score reuse from hash-move usability in the main search, so verified terminal and fail-low entries (move-less by construction) now cut off; made quiescence a TT writer at a reserved draft (`QUIESCENCE_DRAFT` = 0) that the main search can never produce, with its TT block made cutoff-only so every stored bound is classified against the node's own window; extended abort and history-draw write suppression to quiescence; and documented that full-key verification, not move legality, is the identity proof at both probe sites. Verified on c063b0b with cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings on a clean CARGO_TARGET_DIR (no warnings), and cargo test --workspace (291 passed / 0 failed / 2 ignored). Independently confirmed warm-vs-cold score equality and node-count non-increase on eight further positions beyond the four in the suite, and measured the search benchmark base-vs-target interleaved at +0.2% (deadline) and +1.1% (no deadline), well inside the 5% investigate threshold.
<!-- SECTION:FINAL_SUMMARY:END -->
