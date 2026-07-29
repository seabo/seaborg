---
id: TASK-95
title: >-
  En passant treated as empty-target in SEE/qsearch pruning, ordering, and
  valid_move
status: To Do
assignee: []
created_date: '2026-07-29 18:39'
labels:
  - search
  - movegen
dependencies: []
references:
  - engine/src/search.rs
  - chess/src/movegen.rs
priority: medium
type: bug
ordinal: 164000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Several hot-path sites assume the captured pawn sits on an en-passant moves destination square. For en passant it does not: the destination (the ep square) is empty and the victim pawn is on an adjacent square. This one wrong assumption produces a family of defects that mis-value, mis-prune, and mis-order legal en-passant captures. The codebase already has the correct helper -- `captured_piece_type()` (engine/src/search.rs ~3313) special-cases en passant to return Pawn -- but these sites bypass it.

Sites and effects:

1. qsearch SEE / delta / ordering (engine/src/search.rs ~3858 delta cut, ~3876 SEE gate, ~4467 QMoveLoader::score_captures). All read the captured type via `piece_at_sq(mov.dest()).type_of()`, which is `PieceType::None` (value 0) for en passant. SEE then seeds gain[0]=0; after the opponent recaptures the moved pawn it returns roughly -100, so with QUIESCENCE_SEE_THRESHOLD=0 a non-checking en-passant capture is statically pruned (~3889), its delta ceiling is understated by a pawn, and it is ordered as a losing capture. A legal, frequently equal-or-winning pawn capture is dropped from horizon resolution.

2. valid_move (chess/src/movegen.rs 146-152). The capture-flag consistency guard runs before pawn-specific validation: `CAPTURE && (get_occupied_enemy() & dest_bb).is_empty()` is true for every en-passant move (dest empty), so valid_move returns false for a fully legal en-passant move. valid_move validates TT/killer/counter move hints before generation, so an en-passant best move never gets the hint-based early cutoff; per the audit, at engine/src/search.rs ~2294 an en-passant TT move also spuriously sets tt_collision=true, mis-signaling the internal iterative reduction. The in-check path additionally masks ep_square against the evasion target (~689), independently rejecting legal en-passant check evasions.

Played-move correctness is unaffected -- the legal generator still emits en passant and legal_move validates it -- so this is strictly strength (mis-pruning/mis-ordering), not legality. Individually each effect is small (en passant is rare), which is why they are grouped into one correctness-cleanup task with a shared fixture suite rather than separate tickets. Related: TASK-47 (en passant invariants), TASK-49 (SEE promotions). Found via an automated correctness audit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 qsearch SEE, delta pruning, and capture ordering value an en-passant captures victim as a pawn (using the existing captured_piece_type helper), so a SEE-equal or winning en-passant capture is neither statically pruned nor ordered as a losing capture
- [ ] #2 valid_move accepts a legal en-passant move, including when supplied as a TT/killer/counter hint, without returning false or spuriously setting tt_collision; valid_evasion accepts a legal en-passant move that resolves check
- [ ] #3 Regression tests cover an en-passant capture that is (a) SEE-equal and previously statically pruned in qsearch, (b) supplied as a TT hint and previously rejected by valid_move, and (c) a legal en-passant check evasion previously rejected
- [ ] #4 The existing SEE and movegen agreement suites still pass
- [ ] #5 Change measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
