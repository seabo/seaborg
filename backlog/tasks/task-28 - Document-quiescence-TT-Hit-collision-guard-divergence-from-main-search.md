---
id: TASK-28
title: Document quiescence TT Hit collision-guard divergence from main search
status: Done
assignee: []
created_date: '2026-07-17 20:29'
updated_date: '2026-07-19 15:07'
labels:
  - search
  - correctness
dependencies: []
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up from the TASK-9 review (task-9-quiescence-semantics).

The main search validates a TT Hit's stored move with valid_move() as a Zobrist-collision heuristic before trusting the entry (engine/src/search.rs, main alphabeta TT block). The reworked quiescence search now trusts any Probe::Hit on the 16-bit signature alone (sig = zobrist >> 48, plus the index bits) and drops the move-validation guard.

This is a defensible, common design (signature-only verification), and qsearch does not need the TT move for ordering in the early-cutoff path — but it is a deliberate reduction in defensiveness relative to the main search. Without a note, a future reader may either 'restore' the guard or be unaware of the asymmetry. The Clash path is already covered by a regression test.

Decide and document the intended behavior; consider aligning the two functions or adding an explanatory comment at the quiescence TT-cutoff site.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Intended collision-verification behavior for quiescence TT hits is documented at the cutoff site in engine/src/search.rs
- [ ] #2 A decision is recorded on whether to align quiescence and main-search TT verification, or intentionally keep them divergent
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 15:06
---
Closed as delivered by TASK-60, not implemented separately.

TASK-28 asked to decide and document the intended collision-verification behaviour, and to consider aligning quiescence with the main search or adding an explanatory comment. TASK-60 (merged at b0af6b1, approved implementation target c063b0b) did both:

- Aligned the two searches. Neither now requires a playable stored move before trusting a score. The main search's Step 4 cutoff is gated on the verified snapshot alone, and valid_move only decides whether the stored move is usable as an ordering hint; its failure counts a genuine Zobrist collision. Quiescence never consults a stored move at all, because QMoveLoader has no hash phase.
- Documented the decision at both probe sites in engine/src/search.rs, stating that the full-key check inside Table::probe is the identity proof and that move legality is not and never was part of that proof.

The premise of this task is also stale independently of that work. It was written when quiescence trusted a Probe::Hit on a 16-bit signature alone. TASK-57's table rewrite replaced signature matching with full 64-bit key verification inside Table::probe, so the specific reduction in defensiveness described here no longer exists in the form described.

Recorded as Done rather than cancelled because the deliverable does exist in the codebase; the board has no cancelled or superseded status. This closure is administrative: the task had no branch, worktree, or independent review of its own, and was closed on the primary branch at the user's direction after the TASK-60 merge.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Superseded by TASK-60 (merged b0af6b1, approved target c063b0b), which aligned collision-verification behaviour across the main search and quiescence and documented the decision at both probe sites. The task's premise was additionally made stale by TASK-57, which replaced 16-bit signature matching with full 64-bit key verification inside Table::probe. Closed administratively at the user's direction; no separate implementation was required.
<!-- SECTION:FINAL_SUMMARY:END -->
