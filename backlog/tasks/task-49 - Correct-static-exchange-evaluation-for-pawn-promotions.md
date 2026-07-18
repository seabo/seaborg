---
id: TASK-49
title: Correct static exchange evaluation for pawn promotions
status: Done
assignee:
  - '@codex'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-18 22:13'
labels: []
dependencies: []
references:
  - engine/src/see.rs
  - 'http://www.talkchess.com/forum3/viewtopic.php?f=7&t=77787'
priority: medium
type: bug
ordinal: 49000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SEE does not account for pawn promotions. Two cases are wrong: a pawn promoting with a capture, and a pawn promoting without a capture as the first move of the exchange sequence. In both, the material swing includes the promotion gain, which the current swap-off loop ignores, so SEE can misjudge captures involving a pawn on the 7th rank.

The test module in engine/src/see.rs already carries commented-out positions covering this, plus a note suggesting a search extension whenever a pawn is on the 7th rank as an alternative. Work through the linked discussion and decide between correcting the SEE swap loop and extending instead; the ticket is satisfied either way provided the reported values are correct and tested.

Reference: http://www.talkchess.com/forum3/viewtopic.php?f=7&t=77787
TODO site: engine/src/see.rs:134.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 SEE returns the correct value for a pawn capture that promotes
- [x] #2 SEE returns the correct value when a non-capturing promotion is the first move of the exchange
- [x] #3 The currently commented-out promotion positions in the see.rs test module are enabled as asserting tests
- [x] #4 The chosen approach (corrected swap loop vs search extension) is recorded with its rationale in the implementation notes
- [x] #5 The promotion TODO at engine/src/see.rs:134 is removed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce REV-1-01 and trace the pruning state for a capture-promotion followed by recapture.
2. Preserve the existing SEE pruning approximation for ordinary exchanges while preventing it from discarding a recapture of a newly promoted piece.
3. Change the promotion-recapture regression to assert its true +400 material result and run focused SEE tests.
4. Record the finding resolution, run cargo fmt --check, strict workspace Clippy, and cargo test --workspace, then commit a new immutable implementation target and hand it to review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Approach: corrected swap loop (not a search extension)

AC #4. The linked thread proposes either fixing the swap-off algorithm or extending search
whenever a pawn sits on the 7th. I corrected the swap loop, because SEE here is consumed purely
as a move-ordering score in `MoveLoader::score_captures` and `QMoveLoader::score_captures`
(engine/src/search.rs:1118, :1178). A search extension cannot repair an ordering score — it only
spends nodes after the bad ordering has already been applied — whereas the swap-loop fix corrects
the number at source for one rank comparison per attacker selection. Underpromotion is ignored; a
promoting pawn is always valued as a queen, which matches the QueenPromotions-only generator used
by the quiescence loader.

## Model

A pawn arriving on its back rank gains `queen - pawn` material, and the piece it leaves on the
exchange square for the opponent to capture is a queen, not a pawn. Both halves matter; scoring
only the gain still lets the opponent 'win a pawn' on the recapture.

Three changes in engine/src/see.rs:

1. `gain[d]` is the payoff of move *d+1*, so it must carry that move's own promotion gain. The
   loop now selects the next attacker before scoring `gain[d]`. That also moves the occupancy and
   x-ray update ahead of the pruning break, which is harmless because the break discards all
   remaining state, and costs one extra `least_valuable_piece` call on the pruned path.
2. `gain[0]` is seeded from `pos.turn()`, so a promotion on the initial move counts. A
   non-capturing promotion is expressed by passing `PieceType::None` as the target; the existing
   signature already supports this and `piece_value(None) == 0`.
3. `atta_def` clears the origin square with AND-NOT instead of XOR. A pawn pushing to the back
   rank was never an attacker of the target square, so XOR *inserted* a stale bit, which let the
   already-moved pawn be re-selected as an attacker two plies later. Verified load-bearing: with
   the promotion fix but XOR restored, the non-capturing promotion case returns 800 instead of 400.

## Tests (AC #3)

The TODO block carried no commented-out positions, only the note itself — AC #3 is met by turning
that note into asserting cases. Four rows added to `see::tests::it_works`, each verified to fail
against the pre-change algorithm and pass after:

| position | expected | before fix |
|---|---|---|
| `2r5/1P4pk/...` Rxc8 (the thread's reference position) | 500 | 300 |
| `2r5/1P6/...` bxc8=Q, no recapture | 1300 | 500 |
| `r7/4P3/...` e8=Q Rxe8 Rxe8 (non-capturing promotion opens) | 400 | 0 |
| `2rr4/1P6/...` bxc8=Q Rxc8 | 1300 | 500 |

All 20 pre-existing rows are unchanged and still pass.

The last row is filed under the file's existing pruning caveat rather than asserted as a true
value. Its true SEE is 400, but the `max(-gain[d-1], gain[d]) < 0` break fires after the
promotion. This is the same pre-existing approximation the two rows above it already document, not
a promotion-specific defect: given `gain[d] = v - gain[d-1]` with `v >= 0` the break reduces to
`gain[d] < 0`, and it leaves `gain[d-1]` at its speculative value rather than `gain[d-1] - v`.
Promotions widen the gap because `v` is larger, but the sign — whether the initial capture is
deemed favourable — is preserved, which is all the move-ordering caller consumes. Removing the
pruning would change every value in the suite and regress the hot path, so it is out of scope here.

AC #5: the TODO at the old engine/src/see.rs:134 is gone.

## Review rework: promotion recapture cutoff

Resolved REV-1-01. The earlier note and table entry that accepted +1300 for `2rr4/1P6/...` are superseded: the correct value is +400. The ordinary early cutoff is retained for non-promotion exchanges, but is skipped when the standing piece was just promoted. Promotion violates the cutoff's material-ordering assumption because the pawn attacker leaves a queen on the exchange square; retaining the immediate recapture lets the existing minimax fold account for the queen loss. The regression now asserts `bxc8=Q Rxc8` as +400.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-18 20:09
---
Implementation handoff
Branch: task-49-see-promotions
Worktree: /Users/seabo/seaborg-worktrees/task-49-see-promotions
Base: 5b592ebb9f71569007022cff33b03c747484badd
Implementation target: 759846b
Resolved findings: none (first implementation attempt)
Verification:
- cargo fmt --check: pass (clean)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (35 + 159 + 5 + 1 tests, 0 failed, 1 ignored pre-existing)
- discrimination check: each of the four new promotion rows was run against the pre-change swap loop and fails there (500->300, 1300->500, 400->0, 1300->500); the 20 pre-existing rows pass both before and after
Known failures: none

Reviewer note: the fourth new row (2rr4/1P6/...) is deliberately filed under the file's existing
pruning caveat block rather than asserted at its true value of 400. Rationale is in the
implementation notes; the short version is that the pre-existing max(-gain[d-1], gain[d]) < 0
break already distorts magnitudes for two rows above it, promotions only widen the gap, and the
sign the move-ordering caller consumes is preserved. Worth a second opinion on whether that is the
right call for this ticket.
---

author: @codex
created: 2026-07-18 20:17
---
Review attempt: 1
Reviewed branch: task-49-see-promotions
Reviewed implementation: 759846b
Verdict: changes_requested

REV-1-01 [P1] Promotion recapture is pruned before minimax
Location: engine/src/see.rs:88; engine/src/see.rs:201
Impact: A pawn capture-promotion that can be recaptured returns the pre-recapture gain, so SEE reports +1300 for bxc8=Q Rxc8 even though the exchange nets +400. The new test explicitly asserts the known-wrong +1300 value. This leaves AC #1 and the task requirement that reported promotion values be correct unproven.
Reproduction: In the added FEN `2rr4/1P6/8/8/8/6k1/8/6K1 w - - 0 1`, evaluate SEE for b7xc8 with target rook and attacker pawn. The implementation test returns 1300; material accounting is 500 + (900 - 100) - 900 = 400. The implementation notes independently acknowledge 400 as the true result.
Expected: The swap-off/minimax logic must retain the legal ...Rxc8 response and return 400, with the regression test asserting 400 rather than documenting the wrong magnitude as an accepted pruning approximation.

Verification:
- git diff --check 5b592ebb..759846b: pass
- cargo fmt --check: pass
- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (35 core; 159 engine passed, 1 ignored; 5 integration; 1 doc)
- focused inspection of `2rr4/1P6/...` promotion-recapture row: fail, expected 400 but asserted/returned 1300
- cargo bench --bench perft --bench movegen on target and base: no actionable patch signal; these benchmarks do not execute SEE, and sequential runs showed thermal/order variance
---

author: @codex
created: 2026-07-18 21:25
---
Implementation handoff
Branch: task-49-see-promotions
Worktree: /Users/seabo/seaborg-worktrees/task-49-see-promotions
Base: 5b592ebb9f71569007022cff33b03c747484badd
Implementation target: 8f7ff249216ccb2f186ab1d2aeef907b984a6865
Resolved findings: REV-1-01
Verification:
- cargo test -p engine see::tests::it_works: pass (1 passed)
- cargo fmt --check: pass (clean)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (35 core; 159 engine passed, 1 ignored; 5 integration; 1 doc)
Known failures: none
---

author: @claude
created: 2026-07-18 21:51
---
Review attempt: 2
Reviewed branch: task-49-see-promotions
Reviewed implementation: 8f7ff249216ccb2f186ab1d2aeef907b984a6865
Verdict: approved

REV-1-01 resolved. The 2rr4/1P6/8/8/8/6k1/8/6K1 capture-promotion row now asserts +400 and the implementation returns it. Traced by hand: gain[0] = 500 + 800 = 1300, the promotion-aware cutoff retains ...Rd8xc8 so gain[1] = 900 - 1300 = -400, and the minimax fold yields -max(-1300, -400) = 400, matching the material accounting 500 + (900 - 100) - 900 = 400.

Reviewed the full 5b592eb..8f7ff24 diff. Only engine/src/see.rs and the task file changed; no accidental work, no new #[allow], no TODO left in see.rs.

Equivalence for non-promotion exchanges was checked structurally, not just empirically. Hoisting the attacker selection above the gain[d] store leaves gain[d] = value(attacker_d) - gain[d-1] unchanged when no promotion bonus applies; moving the occupancy/x-ray update above the cutoff is inert because the break discards all of occ, atta_def and processed; and AND-NOT equals XOR whenever from_set is a subset of atta_def, which holds for every attacker selected from atta_def and for the initial capturing attacker (including en passant, whose pawn does attack the target diagonally). The one case where they differ is exactly the back-rank push the fix targets.

Acceptance criteria:
- #1 pawn capture that promotes: proven by 2r5/1P6/... at +1300 (no recapture) and 2rr4/1P6/... at +400 (recapture), both matching independent material accounting.
- #2 non-capturing promotion opening the exchange: proven by r7/4P3/... e8=Q Rxe8 Rxe8 at +400.
- #3 the TODO block held only the note, not commented-out positions; it is now four asserting rows including the thread's reference position 2r5/1P4pk/... at +500. Each row was confirmed load-bearing by running it against the base commit's algorithm, where all four fail.
- #4 the corrected-swap-loop choice and its rationale are recorded in the implementation notes.
- #5 the promotion TODO is gone; grep for TODO in engine/src/see.rs returns nothing.

Verification (run in the task worktree; target..HEAD touches only the task file, so code checks on HEAD cover 8f7ff24):
- git diff --check 5b592eb 8f7ff24: clean
- git diff --name-only 8f7ff24 HEAD: task file only
- cargo fmt --check: pass
- CARGO_TARGET_DIR=/tmp/task49-clean-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, zero warnings on a cold target dir
- cargo test --workspace: pass (35 core; 159 engine passed, 1 pre-existing ignored; 5 integration; 1 doc; 0 failed)
- discrimination: scratch worktree at 5b592eb with only the four new rows added, assertions relaxed to prints - all four mismatch (500->300, 1300->500, 400->0, 400->500), the 20 pre-existing rows match

Benchmarks were not run. The diff is confined to see.rs, and Search::see is reached only from MoveLoader/QMoveLoader::score_captures; the perft and movegen benches do not enter search, so they cannot observe this change. The added per-iteration cost is two piece-type/rank comparisons, and the cutoff skip fires only on the ply after a promotion.

Non-blocking observation, no action required: the PieceType::None target path that enables AC #2 is exercised only by tests, since both production callers gate on mov.is_capture(). The function contract is correct and documented; wiring quiet promotions into move ordering would be separate work.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Corrected the SEE swap-off loop to account for pawn promotions instead of adding a search extension, on the grounds that SEE here is consumed only as a move-ordering score. gain[d] is now scored after the next attacker is selected so it carries that move's promotion gain; a promoting pawn leaves a queen on the exchange square; gain[0] seeds the promotion bonus so a non-capturing promotion can open the sequence (PieceType::None target); atta_def clears the origin square with AND-NOT rather than XOR, so a back-rank push no longer inserts a stale attacker; and the max(-gain[d-1], gain[d]) cutoff is skipped for one ply after a promotion so the recapture of the new queen reaches the minimax fold. Four asserting promotion rows replace the TODO note in see::tests::it_works. Verified on 8f7ff24 with cargo fmt --check, clean-CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace (35 core, 159 engine, 5 integration, 1 doc; 0 failed), plus a base-commit discrimination run showing all four new rows fail on the pre-change algorithm (500->300, 1300->500, 400->0, 400->500) while the 20 pre-existing rows are unaffected.
<!-- SECTION:FINAL_SUMMARY:END -->
