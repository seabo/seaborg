---
id: TASK-49
title: Correct static exchange evaluation for pawn promotions
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-18 20:09'
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
- [ ] #1 SEE returns the correct value for a pawn capture that promotes
- [ ] #2 SEE returns the correct value when a non-capturing promotion is the first move of the exchange
- [ ] #3 The currently commented-out promotion positions in the see.rs test module are enabled as asserting tests
- [ ] #4 The chosen approach (corrected swap loop vs search extension) is recorded with its rationale in the implementation notes
- [ ] #5 The promotion TODO at engine/src/see.rs:134 is removed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Decide approach: correct the SEE swap loop rather than add a search extension. SEE is consumed as a move-ordering score in Search::score_captures/QMoveLoader::score_captures, where an extension cannot help; the swap-loop fix is local and costs one rank comparison per attacker selection.
2. Model promotion in engine/src/see.rs::see as: a pawn arriving on its back rank gains (queen - pawn) material, and the piece it leaves on 'to' for the opponent to capture is a queen, not a pawn.
3. Restructure the swap loop so the next attacker is selected before gain[d] is computed, since gain[d] is the payoff of move d+1 and must include that move's promotion bonus.
4. Seed the first ply from Search::pos.turn() so a promotion on the initial move (capturing or non-capturing) is counted in gain[0].
5. Clear the origin square from atta_def with AND-NOT instead of XOR. For a non-capturing promotion the pawn is not an attacker of 'to', so XOR would insert a stale bit and let the moved pawn be re-selected later.
6. Replace the TODO block in the see.rs test module with asserting promotion cases: the Arasan reference position from the linked thread, an initial capture-promotion, and a non-capturing promotion opening the exchange. Add any pruning-distorted case to the existing documented pruning block rather than asserting a wrong value silently.
7. Run cargo fmt --check, strict clippy, and cargo test --workspace.
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
<!-- COMMENTS:END -->
