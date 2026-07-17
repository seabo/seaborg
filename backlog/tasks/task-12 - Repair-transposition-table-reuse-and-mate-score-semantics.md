---
id: TASK-12
title: Repair transposition-table reuse and mate-score semantics
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 23:22'
labels:
  - search
  - tt
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/tt.rs
priority: high
type: bug
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search unconditionally clears the shared transposition table because of a known PVS interaction, preventing reuse and undermining concurrent workers. Mate scores also need ply-aware storage and retrieval so transpositions preserve distance-to-mate ordering.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Starting a normal search does not unconditionally invalidate the shared transposition table
- [ ] #2 New-game and explicit clear operations have documented ownership and generation behavior
- [ ] #3 Mate scores are encoded and decoded relative to ply so transposed positions preserve mate distance
- [ ] #4 Concurrent search workers do not invalidate one another through table generation changes
- [ ] #5 Tests cover reuse across searches, explicit clear, transposed mate scores at different plies, and concurrent probes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Resolve REV-1-01: this engine uses a POSITION-RELATIVE mate convention (checkmate leaf returns constant Score::mate(0); inc_mate accumulates distance on unwind), so a node's mate score is the distance-from-that-position and is invariant to the ply at which the position is reached. The Stockfish-style +ply/-ply TT adjustment is only valid for a root-relative convention and corrupts distances on cross-ply transpositions. Revert it: remove Score::to_tt/from_tt and the ply plumbing threaded through WritableEntry::write/read and quiesce/quiesce_evasions, restoring identity (position-relative) TT storage that existed at the base.
2. Preserve the legitimate task improvements: removed unconditional Search::run tt.clear(); SearchEngine::clear_hash/new_game ownership; UCI ucinewgame and GameController reset wiring that stops workers before advancing generation; reuse/concurrency tests.
3. Tests: replace the incorrect mate_scores_decode_relative_to_the_probe_ply / tt_mate_scores_are_adjusted_for_probe_ply tests with assertions that a stored mate score round-trips through the TT unchanged (position-relative distance is preserved for a probe at any ply). Retain reuse/explicit-clear/concurrent-probe tests and the gives_correct_answers mate suite (now exercises a warm TT across iterative-deepening iterations).
4. Run cargo fmt --check and cargo test --workspace; commit implementation; record Resolved REV-1-01 and a fresh In Review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Removed per-search generation advancement and made SearchEngine owners responsible for explicit clear/new-game boundaries. UCI new-game and GameController resets now stop active work before advancing the shared generation. TT reads/writes encode and decode mate scores using the caller’s ply in both main and quiescence search paths. Added sequential reuse, explicit clear, new-game handling, cross-ply mate, concurrent probe, and concurrent search regressions.

Validation passed:
- cargo fmt --check
- cargo test --workspace

Rework (attempt 2): Resolved REV-1-01. This engine scores mate POSITION-RELATIVE — the checkmate leaf returns a constant Score::mate(0) (search.rs) and inc_mate accumulates distance-to-mate on unwind, so a node's mate score is the distance from that position and is invariant to the ply at which the position is reached. The prior ply-relative (Stockfish-style +ply/-ply) TT adjustment was only valid for a root-relative convention; applied here it made the same position report different mate distances by probe ply and wrote scores outside the [-20_100, 20_100] mate range. Removed Score::to_tt/from_tt and reverted the ply plumbing threaded through WritableEntry::write/read and quiesce/quiesce_evasions back to the base position-relative storage. Removing the unconditional Search::run tt.clear() means iterative-deepening iterations now reuse a warm TT, so gives_correct_answers additionally exercises mate scores read from a warm table across iterations. Replaced the incorrect mate_scores_decode_relative_to_the_probe_ply / tt_mate_scores_are_adjusted_for_probe_ply tests with mate_scores_are_stored_position_relative, asserting a stored mate distance round-trips unchanged. Preserved all other task work (clear_hash/new_game ownership, UCI ucinewgame and GameController reset wiring, reuse/concurrency tests).

Verification: cargo fmt --check passed; cargo test --workspace passed (engine lib 59 passed/1 ignored, engine integration 31 passed, build_metadata 5 passed, doc-tests passed).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 21:44
---
Implementation handoff
Branch: task-12-tt-reuse-mate-scores
Worktree: /Users/seabo/seaborg-worktrees/task-12-tt-reuse-mate-scores
Base: 2c3a91b42c8810ca1897c4fc7675470aa4245ac0
Implementation target: 2b1fe1624d1c9ea92f4638a1da619256699032c8
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo test --workspace: passed (91 passed, 1 ignored)
Known failures: none
---

author: @codex
created: 2026-07-17 23:07
---
Review attempt: 1
Reviewed branch: task-12-tt-reuse-mate-scores
Reviewed implementation: 2b1fe1624d1c9ea92f4638a1da619256699032c8
Verdict: changes_requested

REV-1-01 [P1] Ply-relative TT mate encoding corrupts this engine's position-relative mate scores
Location: engine/src/score.rs:94-113 (to_tt/from_tt); applied at engine/src/tt.rs:290 (write) and engine/src/tt.rs:301 (read); consumed at engine/src/search.rs:506,531,536 and quiesce probes.
Impact: Blocks AC#3 ("transposed positions preserve mate distance") and the AC#5 mate test. This engine scores mate position-relative, not root-relative: the checkmate leaf returns a constant Score::mate(0) independent of ply (search.rs:716, and quiesce_evasions at :915), and inc_mate accumulates distance-to-mate on unwind (search.rs:645,658,891,927). A value such as mate(1) at a node is therefore an intrinsic property of that position and is identical no matter the ply at which the position is reached. The added Stockfish-style +ply/-ply adjustment is only correct for a root-relative convention. Applied here it makes the SAME position report different mate distances depending on probe ply, which is the exact opposite of AC#3, and it writes raw scores outside the documented mate range [-20_100, 20_100]. On a genuine cross-ply TT transposition the decoded score returned from Step 3/Step 4 (search.rs:506/531/536) is wrong, so search can return an incorrect mate distance/cutoff. The pre-existing unadjusted store/read was already correct for these position-relative scores; this change is a regression.
Reproduction: Standalone reproduction mirroring the engine's exact Score encoding and the new to_tt/from_tt:
  - A position intrinsically mate(1): to_tt at write-ply 2 -> raw 20101 (OUT OF the valid 20_000..=20_100 mate range).
  - from_tt at read-ply 2 (same ply) -> mate(1) (round-trips, so same-ply hits mask the bug).
  - from_tt at read-ply 4 (transposition) -> mate(3): the same position now reports a different mate distance.
  The enshrined unit test tt.rs mate_scores_decode_relative_to_the_probe_ply (writes mate(7)@ply3, asserts read(5)==mate(9)) and score.rs:249 assert the incorrect behavior as if correct. The full suite passes only because gives_correct_answers uses a fresh table per position and its forced mate lines do not exercise a cross-ply transposition of a mate-scored node.
Expected: Because search values are position-relative, TT mate scores must survive an arbitrary-ply probe unchanged: entry.read(any_ply) for a stored mate(n) must return mate(n). Fix by storing/retrieving mate scores without ply adjustment (identity to_tt/from_tt), or, if ply-relative TT storage is genuinely desired, convert the ENTIRE search to a root-relative mate convention (ply-dependent mate leaf + matching pruning) so the pieces are consistent. Update the mate tests to assert distance preservation across differing probe plies.

Verification:
- cargo test --workspace: passed (60 + 5 + docs; masks the latent bug per above)
- cargo fmt --check: passed
- standalone Score-encoding reproduction (rustc -O): intrinsic mate(1) written@ply2 -> raw 20101 (out of range); read@ply2 -> mate(1); read@ply4 -> mate(3) [same position, different mate distance]
---

author: @codex
created: 2026-07-17 23:22
---
Resolved REV-1-01 [P1]: TT mate encoding is now position-relative (identity), matching this engine's constant-mate-leaf + inc_mate convention, so a stored mate distance is returned unchanged for a probe at any ply and cross-ply transpositions preserve mate distance. Removed Score::to_tt/from_tt and the ply plumbing; scores are stored verbatim. Behavior change verified by mate_scores_are_stored_position_relative and the warm-TT gives_correct_answers mate suite (exact mate(5)/mate(7) distances), plus cargo test --workspace and cargo fmt --check.

Implementation handoff
Branch: task-12-tt-reuse-mate-scores
Worktree: /Users/seabo/seaborg-worktrees/task-12-tt-reuse-mate-scores
Base: 2c3a91b42c8810ca1897c4fc7675470aa4245ac0
Implementation target: de1ccb9f1092c9b4b9ba649a02b67452c37c61ed
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: passed
- cargo test --workspace: passed (engine lib 59 passed, 1 ignored; engine integration 31 passed; build_metadata 5 passed; doc-tests passed)
Known failures: none
---
<!-- COMMENTS:END -->
