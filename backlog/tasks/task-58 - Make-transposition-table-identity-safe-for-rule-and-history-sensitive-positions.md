---
id: TASK-58
title: >-
  Make transposition-table identity safe for rule- and history-sensitive
  positions
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 02:21'
labels:
  - transposition-table
  - zobrist
  - search
  - correctness
  - rules
dependencies: []
references:
  - core/src/position/zobrist.rs
  - core/src/precalc/zobrist.rs
  - engine/src/search.rs
priority: high
type: bug
ordinal: 57000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Zobrist key identifies board state, side to move, castling rights, and en-passant file, but search values also depend on the halfmove clock and potentially on repetition history. Static evaluation is explicitly scaled by the halfmove clock, so identical keys can currently carry different values. Establish and enforce a documented TT-reuse policy for halfmove-clock and repetition-sensitive results. Also canonicalise en-passant hashing so an unusable target does not split positions with identical legal state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A warm-table search cannot reuse a score or bound computed under an incompatible halfmove-clock state
- [ ] #2 The treatment of repetition-dependent results is documented and enforced so history-sensitive draw outcomes cannot be reused as position-intrinsic exact information in an incompatible history
- [ ] #3 Positions that differ only by an en-passant target which cannot affect any legal move have the same canonical transposition identity, while a legally relevant en-passant right remains distinguished
- [ ] #4 Regression tests cover warm-table reuse at materially different halfmove clocks, compatible and incompatible repetition histories, and capturable versus non-capturable en-passant targets
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. AC#1 halfmove clock: remove the halfmove-clock scaling from Search::evaluate() (search.rs:910) so static eval is position-intrinsic. This makes TT reuse sound with respect to the clock by construction rather than by gating reuse on a clock stored in the packed 64-bit Entry (which has no free bits and would cost hit rate). Document the invariant at evaluate() and at the TT write site. Fifty-move handling remains in the draw detection, which the search discovers within its own horizon.

2. Fix the inconsistent quiescence fifty-move threshold: quiesce() uses half_move_clock() >= 50 (search.rs:950) while the main search uses fifty_move_rule_reached() == 100 plies (search.rs:596). The quiescence check fires at 25 moves and reports a false draw. Use fifty_move_rule_reached() in both.

3. AC#2 repetition policy: draw short-circuits at search.rs:596 and 950 return before the TT write at search.rs:852, so a directly repetition-derived draw is never itself stored. The remaining hazard is a repetition draw propagating up into an ancestor's score, which is then stored as position-intrinsic. Enforce with a monotone counter on Search incremented at each repetition draw short-circuit: a node samples it before searching children and compares after, and if it increased the node's value is history-contaminated and must not be written as Bound::Exact. Document the policy as the TT-reuse contract.

4. AC#3 en-passant canonicalisation: make_move_unchecked (core/src/position/mod.rs:325) already sets ep_square only when an enemy pawn pseudo-legally attacks it, but from_fen accepts any ep square with no board reconciliation (TODO at fen.rs:450). A FEN-parsed position and the same position reached by moves therefore get different Zobrist keys. Apply the same enemy-pawn-attacks predicate in from_fen after the bitboards are built and before set_zobrist(), so the canonical key is correct by construction. Full legality filtering (pinned capturer, ep-discovered-check) is deliberately out of scope: it needs legality checks inside make_move on the hot path.

5. AC#4 regression tests: warm-table reuse at materially different halfmove clocks; compatible vs incompatible repetition histories; capturable vs non-capturable en-passant targets sharing or splitting identity; FEN-parsed vs move-reached key agreement. Replace the existing material_evaluation_scales_over_one_hundred_halfmoves test, whose asserted scaling is being removed.

6. Run cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace.
<!-- SECTION:PLAN:END -->
