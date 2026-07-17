---
id: TASK-28
title: Document quiescence TT Hit collision-guard divergence from main search
status: To Do
assignee: []
created_date: '2026-07-17 20:29'
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
