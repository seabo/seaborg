---
id: TASK-49
title: Correct static exchange evaluation for pawn promotions
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-18 20:04'
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
