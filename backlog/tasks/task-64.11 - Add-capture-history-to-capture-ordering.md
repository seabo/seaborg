---
id: TASK-64.11
title: Add capture history to capture ordering
status: Done
assignee:
  - '@george'
created_date: '2026-07-19 13:33'
updated_date: '2026-07-21 18:45'
labels:
  - search
  - move-ordering
  - see
dependencies:
  - TASK-64.2
  - TASK-64.17
references:
  - engine/src/ordering.rs
  - engine/src/history.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 74000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Captures are ordered purely by static exchange evaluation, with no learned component. Add a capture history table so that captures the search has found to cause cutoffs are tried earlier among captures of equal material outcome.

Current state. Both capture scorers assign the raw SEE value as the ordering score (search.rs:1472-1486 and search.rs:1532-1546). The phase machinery then splits captures into GoodCaptures (SEE greater than zero), EqualCaptures (SEE equal to zero) and BadCaptures (SEE less than zero) via predicate filters over one generated segment (ordering.rs:580-624).

SEE answers only whether a capture wins material on that square. It cannot distinguish between several captures with identical material outcomes, and the EqualCaptures phase in particular is currently ordered arbitrarily within itself. A capture history table, conventionally keyed on moving piece, destination square and captured piece type, supplies the missing signal.

Scope note. This is deliberately separate from the counter-move and continuation history work so that the two can be measured independently; capture ordering and quiet ordering fail in different ways and a combined measurement would not attribute either. It shares the bonus, malus and aging scheme established by the history activation task, which is why it depends on it.

An open question to settle and record: whether capture history should adjust ordering within the existing SEE-derived phases only, or whether it should be able to promote a capture across a phase boundary. The former is more conservative and preserves the material-based phase guarantee that other work in this programme relies on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A capture history table is maintained and contributes to capture ordering scores
- [x] #2 The decision on whether capture history can move a capture across an SEE-derived phase boundary is recorded and implemented
- [x] #3 Bonus, malus and aging follow the scheme established for plain history
- [x] #4 A test asserts that among captures with equal SEE, one previously causing cutoffs is ordered first
- [x] #5 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a CaptureHistory table in history.rs keyed on (moving piece, destination square, captured piece type), backed by a boxed flat i32 slice and using the shared gravity_update rule for bonus/malus/aging. En-passant captures key their captured type as Pawn so read and update stay consistent.
2. Wire it into Search: new field, construction in Search::new, and reset in the per-search reset block alongside the other move-ordering tables.
3. Train it on beta cutoffs: track failed captures in the main move loop; on any cutoff apply a depth-scaled malus to failed captures and, when the cutoff move is itself a capture, a bonus to it. New update_capture_histories mirrors update_quiet_histories.
4. Decision (AC#2): capture history adjusts ordering WITHIN the existing SEE-derived phases only and can never move a capture across a phase boundary. Implemented by partitioning the capture segment on pure SEE first (unchanged), then folding a bounded history term into the within-phase ordering score. The term is bounded below one pawn (the minimum nonzero SEE granularity, since piece values are whole pawns), so it breaks ties among captures of equal material outcome without ever outweighing a material difference. This preserves the material-based phase guarantee that move-count pruning and LMP depend on.
5. Ordering plumbing: add Loader::score_capture_history (default no-op); call it in ordering.rs after the SEE partition commits the good/equal/bad subranges; implement it on MoveLoader to add the bounded history term to the stored SEE score. Quiescence (QMoveLoader) keeps pure SEE.
6. Tests: capture-history bounded/aging/keying test in history.rs; a search.rs test asserting that among captures of equal SEE the one previously causing cutoffs is ordered first.
7. Run cargo fmt/clippy/test and the TASK-27 strength-regression script; record results in implementation notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented capture history for capture ordering.

New table (engine/src/history.rs): CaptureHistory, a boxed 12 x 64 x 6 grid of
i32 keyed on (moving piece, destination square, captured piece type). It shares
the existing bounded gravity_update rule with the quiet tables, so bonus, malus
and aging are identical (AC#3). En-passant captures key their captured type as a
pawn so read and update agree. Wired into Search as a search-local field,
constructed in Search::new and cleared in the per-search reset block alongside the
other move-ordering tables.

Training (engine/src/search.rs, update_capture_histories): on a beta cutoff the
cutoff move receives a depth-scaled bonus when it is itself a capture, and every
capture that was searched and failed before it (tracked in failed_captures)
receives the matching malus — applied whether the cutoff was a capture or a quiet,
so a searched-but-failed capture is penalised regardless of what refuted the node.

Ordering contribution (AC#1): score_capture_history is a new Loader hook called
from ordering.rs immediately after the static-exchange partition commits the good,
equal and bad capture subranges. It reads each capture's current score (its SEE
value) and adds a bounded history term.

Decision on phase-boundary crossing (AC#2): capture history adjusts ordering WITHIN
the existing SEE-derived phases only; it can never promote a capture across a phase
boundary. Rationale: move-count pruning and late-move reduction rely on the
material-based split, so it is preserved. Implementation: the SEE-sign partition
runs first and unchanged; the history term is folded in only afterwards, and it is
bounded to CAPTURE_HISTORY_ORDER_MAX = PAWN_VALUE/2 - 1 = 49. Because every SEE
outcome is a whole multiple of the pawn value (100), the largest possible swing
between two captures (2*49 = 98) cannot bridge a one-pawn gap, so history only
breaks ties among captures of identical material outcome and never reorders
captures of different value. Quiescence (QMoveLoader) keeps pure SEE ordering; the
change is confined to the main search.

Tests:
- history.rs capture_history_updates_are_bounded_and_key_local: bonus/malus/aging
  bounds and key isolation (mover, destination, captured type).
- search.rs trained_captures_break_ties_among_equal_static_exchange_value (AC#4):
  two pawn captures of equal SEE; the one trained with cutoff history is ordered
  ahead of the one penalised, reversing generation order.
- Existing ordering_buffer_worst_case_occupancy test exercises the king plane via
  its illegal synthetic maximum-mobility position (white can "capture" the enemy
  king there); the table reserves that plane so the debug-asserted index stays in
  range.

Strength measurement (AC#5), TASK-27 script, bounded / NON-AUTHORITATIVE run:
- baseline daa79cb (merge-base) vs candidate implementation, both release
  target-cpu=native; FastChess alpha 1.5.0; tc=4+0.04; concurrency 6; Hash 16;
  Threads 1; openings seaborg-openings-v1; paired colour reversal; max-games 400.
- Result: 400 games, W103 D169 L128, Elo -21.74 +/- 26.85, LLR -0.41 inside SPRT
  bounds [-2.94, 2.94] (elo0=-5, elo1=0, alpha=beta=0.05). Verdict INCONCLUSIVE;
  0 forfeits, 0 crashes. Report archived at /tmp/strength-64.11/report.json.
- Interpretation: the point estimate leans slightly negative but its error far
  exceeds it, so a 400-game cap cannot resolve a small move-ordering effect. A full
  authoritative SPRT (commonly thousands of games) is required for a strength
  verdict and is left to the merge-time gate / a dedicated run.
- Caveat: the run was executed before the implementation was committed, so the
  report labels the candidate git id 702fd91 (the claim commit). The candidate
  binary bytes are the full implementation (sha256 05470f73...), whose source is
  byte-identical to the committed target 8c75e5d.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-21 17:23
---
Implementation handoff
Branch: task-64.11-capture-history
Worktree: /Users/seabo/seaborg-worktrees/task-64.11-capture-history
Base: daa79cb8a19d635702e894927f44064e76480f95
Implementation target: 8c75e5d7f744bf6d18eebdf7e80957733096f760
Resolved findings: none
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (no warnings)
- cargo test --workspace: PASS (all suites green; engine lib 393 passed, 2 ignored)
- TASK-27 strength script, bounded NON-AUTHORITATIVE run (tc=4+0.04, 400 games, baseline daa79cb): INCONCLUSIVE, Elo -21.74 +/- 26.85, LLR -0.41 within SPRT bounds; 0 crashes/forfeits. A full authoritative SPRT is still required for a strength verdict.
Known failures: none
---

author: @george
created: 2026-07-21 17:35
---
Review attempt: 1
Reviewed branch: task-64.11-capture-history
Reviewed implementation: 8c75e5d7f744bf6d18eebdf7e80957733096f760
Verdict: approved

Immutability: base daa79cb is the merge-base with master and an ancestor of the target; target 8c75e5d is an ancestor of tip d9270a5; the only post-target commit touches solely the task file (handoff metadata). base..target touches exactly history.rs, ordering.rs, search.rs and the task file — no unrelated changes, no new #[allow].

Acceptance criteria (all proven):
- AC#1: CaptureHistory maintained (constructed in Search::new, reset per search) and contributes via the new score_capture_history Loader hook.
- AC#2: decision recorded and implemented — intra-phase tie-break only; the SEE-sign partition is committed before the history term is folded in, so a capture can never move across a phase boundary; the +/-49 bound (< half a pawn) cannot bridge the >=100 gap between distinct whole-pawn SEE outcomes. Index bounds verified safe including the reserved king plane (max index 4607 < 4608).
- AC#3: bonus/malus/aging reuse the shared gravity_update rule; verified by capture_history_updates_are_bounded_and_key_local (saturation to +/-HISTORY_MAX, key isolation, reset).
- AC#4: trained_captures_break_ties_among_equal_static_exchange_value passes — two equal-SEE pawn captures reordered by history alone.
- AC#5: TASK-27 strength script run and results recorded in the notes (400 games, Elo -21.74 +/- 26.85, INCONCLUSIVE); the AC requires measurement recorded, which is satisfied. Authoritative SPRT deferred to the merge gate.

Correctness spot-checks: position is the node position at both scoring (no move made) and update (move unmade at Step 21 before the cutoff block); en-passant keys its captured type as Pawn consistently on read and update; the modified ordering score is consumed only by the within-range SelectionSort and never leaks to SEE pruning (which recomputes see()).

Benchmarks: perft/movegen benches do not exercise the search-ordering path this diff touches, and the added cost is a single bounded table read per capture, dominated by the SEE already computed; the search-speed net is the AC#5 strength run.

Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): PASS, no warnings
- cargo test --workspace: PASS (engine lib 393 passed, 2 ignored; workspace green)
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds a bounded CaptureHistory table (12 movers x 64 squares x 6 captured types) keyed on (moving piece, destination, captured type), trained on beta cutoffs and folded into capture ordering as an intra-phase tie-break. Decision on AC#2: capture history never crosses an SEE-derived phase boundary — the good/equal/bad partition is committed first, then a history term bounded to +/-49 (< half a pawn) is folded in, and since every SEE outcome is a whole multiple of the pawn value (100), the max 2*49=98 swing cannot bridge a one-pawn gap, so history only breaks ties among equal-material captures. Bonus/malus/aging reuse the shared gravity_update rule (AC#3). Verified: cargo fmt --check PASS; cargo clippy --workspace --all-targets --all-features -D warnings PASS on a clean CARGO_TARGET_DIR; cargo test --workspace PASS (engine lib 393 passed/2 ignored) including the two new tests capture_history_updates_are_bounded_and_key_local and trained_captures_break_ties_among_equal_static_exchange_value (AC#4). Strength measured with the TASK-27 script and recorded in the notes (AC#5): 400-game bounded run, Elo -21.74 +/- 26.85, INCONCLUSIVE within SPRT bounds — the point estimate is dwarfed by its error and an authoritative SPRT is deferred to the merge-time gate.
<!-- SECTION:FINAL_SUMMARY:END -->
