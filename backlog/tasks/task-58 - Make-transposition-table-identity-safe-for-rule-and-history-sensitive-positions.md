---
id: TASK-58
title: >-
  Make transposition-table identity safe for rule- and history-sensitive
  positions
status: Changes Requested
assignee:
  - '@codex'
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 02:42'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Made the static evaluation position-intrinsic (engine/src/search.rs). The previous halfmove-clock scaling made every propagated score a function of state the Zobrist key does not cover, which is the direct cause of AC#1: a warm table could return a score computed under a materially different clock. Removing the scaling makes the invariant hold by construction. The alternative considered was storing a clock in the packed 64-bit TT Entry and gating reuse; Entry has no free bits, and a strict equality gate would reject most reuse while a tolerance band has no clean correctness story. Approach confirmed with the user before implementing.

Suppressed the TT write for any node whose subtree claimed a history-sensitive draw (search.rs Step 24), tracked by a monotone history_draws counter incremented in the new is_history_draw helper. Downgrading Exact to a bound is insufficient and the comment at the write site records why: a draw score can raise a value to a beta cutoff as easily as it can cap it, so Lower and Upper are unsound in an incompatible history too. Verified the policy is not a blanket suppression: a_history_independent_value_is_still_stored_in_the_table asserts ordinary values still reach the table.

Canonicalised the en-passant target on FEN input (core/src/position/fen.rs, canonicalize_ep_square), applying the same enemy-pawn-attacks predicate make_move_unchecked already uses, and running it before set_zobrist so the canonical square is the one hashed. This closes the pre-existing TODO at fen.rs. Note AC#3 was narrower than the task assumed: make_move_unchecked already declined to set an unusable target, so the real gap was FEN input. Full legality filtering (pinned capturer, ep-discovered-check) is deliberately out of scope, as it requires legality checks inside make_move on the hot path; scope confirmed with the user.

Found and fixed two defects while establishing the above.

First, Zobrist::from_position folded a Piece::None key into every empty square, because iterating Board yields all 64 squares including empty ones, while the incremental updates in make_move_unchecked only ever toggle real pieces. The two derivations disagreed by the Piece::None keys of every square whose occupancy changed. Confirmed the divergence reproduces at the base commit on unmodified master, and confirmed the delta numerically equals piece_square_key(None, orig) xor piece_square_key(None, dest). This was a blocking prerequisite for AC#3 rather than opportunistic scope: the en-passant identity guarantee is asserted across exactly the parsed-versus-played boundary that this bug split, so AC#3 is untestable without it. Added incremental_and_full_keys_agree_after_every_legal_move and unmaking_a_move_restores_the_key over quiet moves, captures, castling, promotions and en-passant.

Second, quiescence compared the halfmove clock against 50 while the main search used the correct 100-ply boundary, so quiescence reported a draw at 25 moves. Both now route through is_history_draw.

Behavioral note for review: removing the eval scaling changes engine playing behavior in drawish endings, which is intended but strength-relevant. The existing quiescence_searches_quiet_check_evasions expectation moved from -495 to -500; the -495 was the clock smear (the evasion search advanced the clock to 1 and shaded a -500 rook by one percent), so -500 is the corrected value. No benchmark was run: the ACs do not require one and I did not want to record an uncontrolled measurement, so nps attribution is left to the merge-time gate.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 02:36
---
Implementation handoff
Branch: task-58-tt-identity-rule-sensitive
Worktree: /Users/seabo/seaborg-worktrees/task-58-tt-identity-rule-sensitive
Base: 5ce2782948b96d070404dc73b0a91c89330a3709
Implementation target: 99e9b4f
Resolved findings: none (first review)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 40 core + 193 engine + 5 build_metadata + 1 doc test, 0 failed, 2 ignored (pre-existing)
Known failures: none

Reviewer notes:
- Two design decisions were confirmed with the user before implementing: satisfy AC#1 by removing the halfmove-clock scaling from evaluate() rather than gating TT reuse on a stored clock, and scope AC#3 to FEN reconciliation rather than full en-passant legality filtering. Both rationales are in the implementation notes.
- The change to Zobrist::from_position alters every Zobrist key in the engine. It is a fix for a real divergence between the full and incremental key derivations that reproduces on unmodified master, and is a prerequisite for AC#3 rather than opportunistic scope. Worth independent confirmation that it is correctly in scope for this task.
- Removing the eval scaling is a strength-relevant behavior change in drawish endings. No benchmark was run; nps and strength attribution is left to the merge-time gate.
---

author: @codex
created: 2026-07-19 02:42
---
Review attempt: 1
Reviewed branch: task-58-tt-identity-rule-sensitive
Reviewed implementation: 99e9b4f
Verdict: changes_requested

REV-1-01 [P1] Warm TT reads remain incompatible with halfmove clock and repetition history
Location: engine/src/search.rs:629-678 and engine/src/search.rs:1007-1044
Impact: AC#1 and AC#2 remain unsatisfied. The patch suppresses writes only after the current search observes a history-sensitive draw, but an entry written by a low-clock or history-independent search is accepted solely by key, depth, and bound. A later incompatible visit can return it before searching the subtree that would claim the draw.
Reproduction: Warm the table at halfmove clock 0, then search the same keyed position near clock 100 deeply enough that a reversible continuation reaches the boundary. The existing clock-80/depth-4 test never reaches incompatibility. The same ancestor-level issue applies when another repetition history makes a descendant draw relevant.
Expected: Gate or key TT reads so scores and bounds are reused only in compatible rule/history state, with material incompatible-history regressions.

REV-1-02 [P1] En-passant canonicalization retains legally unusable targets
Location: core/src/position/fen.rs:113-132
Impact: AC#3 requires equal identity when a target cannot affect any legal move. The pawn-attack predicate retains a target when the sole capturer is pinned or the capture exposes its king.
Reproduction: Compare "k3r3/8/8/3pP3/8/8/8/4K3 w - d6 0 1" with the same FEN using "-" for en passant. e5xd6 exposes the rook check and is illegal, yet the target is retained and hashed.
Expected: Retain the target only when at least one legal en-passant capture exists, with pinned/discovered-check regression coverage.

Verification:
- cargo fmt --check: pass
- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (40 core, 193 engine, 5 build_metadata, 1 doc; 2 ignored)
- git diff --check base..target: pass
- benchmarks not run because another repository benchmark consumed about 90% CPU, preventing a valid idle-machine comparison
---
<!-- COMMENTS:END -->
