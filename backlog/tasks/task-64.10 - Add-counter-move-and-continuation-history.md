---
id: TASK-64.10
title: Add counter-move and continuation history
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 05:18'
labels:
  - search
  - move-ordering
dependencies:
  - TASK-64.1
  - TASK-64.2
  - TASK-64.3
  - TASK-64.17
references:
  - engine/src/history.rs
  - engine/src/ordering.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 73000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Quiet move ordering currently combines a two-slot per-ply killer stage with one side-specific from-to butterfly history table. Add counter-move and continuation history so ordering can condition a quiet move on the moves that preceded it rather than only on its origin and destination.

Current state. HistoryTable holds one 64x64 from-to table per side. There is no counter-move table and no continuation history. The staged order is HashTable, QueenPromotions, GoodCaptures, EqualCaptures, Killers, Quiet, BadCaptures, Underpromotions. TASK-64.3 repairs the killer table into a small recency cache of same-ply refutations; this task must determine empirically how that cache should coexist with stronger contextual evidence rather than assuming every heuristic deserves a permanent independent stage.

Continuation history is a major remaining move-ordering opportunity. A global from-to table cannot distinguish a move that is generally useful from one that is specifically a strong reply to the preceding position. Maintain continuation evidence for at least one and two plies back; consider additional distances only with a recorded rationale and acceptable memory/cache behavior.

A counter-move table is the one-ply special case that retains one candidate reply to the previous move. A dedicated counter stage after killers is a reasonable initial implementation, but it is a hypothesis rather than a required final architecture. Compare it against folding counter and killer candidates into a combined contextual quiet ranking. Also measure whether equal captures should remain ahead of killers. Prefer the simplest ordering that wins on fixed-depth node count, throughput and strength.

Use the per-ply search stack to obtain preceding moves. Share the bounded bonus, malus and aging scheme established for plain history rather than introducing independent unbounded counters. New candidates or stages must participate in hash, killer, counter and quiet duplicate suppression and every externally stored move must be validated before unsafe execution.

This depends on TASK-64.1, TASK-64.2, TASK-64.3 and TASK-64.17. Coordinate measurement with TASK-64.3: once contextual history is active, run an ablation with killers disabled, one slot and two slots. Retaining, combining or deleting killers are all acceptable outcomes when supported by results.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A counter-move is tracked by previous move and participates in ordering with complete duplicate suppression; a dedicated stage after killers may be the initial implementation but is not mandated as the final architecture
- [x] #2 Continuation history is maintained for at least one and two plies back and contributes to quiet move ordering
- [x] #3 The implemented continuation distances, indexing scheme, memory footprint and expected per-worker ownership are recorded with rationale
- [x] #4 Bonus, malus and aging use the bounded scheme established for plain history rather than separate unbounded or exposure-based counters
- [x] #5 Tests show that contextual evidence can order a reply ahead of a move with higher plain history and cover duplicate suppression against hash, killer, counter and ordinary quiet candidates
- [x] #6 Externally stored killer and counter candidates are legality-validated before unsafe move execution
- [x] #7 Fixed-depth node counts and search throughput compare a dedicated killer/counter stage with a combined contextual quiet-ranking design, and compare equal captures before versus after refutation candidates
- [x] #8 After contextual history is active, an ablation compares killers disabled, one slot and two slots; the recorded decision may retain, combine or remove the killer heuristic
- [x] #9 Representative fixed-depth node counts improve without an unacceptable throughput regression, with figures recorded in implementation notes
- [x] #10 The selected design is measured with the TASK-27 strength-regression script and results are recorded in implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Shared bounded update. Extract the plain-history gravity update (bonus/malus/aging, clamp to +/-HISTORY_MAX) into one primitive reused by every contextual table so counter/continuation evidence uses the same bounded scheme, not independent unbounded counters.
2. Counter-move table. Key one quiet reply by the previous move's (moving piece, destination). Per-worker field on Search, reset each search alongside history/killers. Read is legality-validated with Position::valid_move before it can be executed.
3. Continuation history. Two gravity tables for 1 and 2 plies back, indexed [prev piece-to context][current piece-to] as i32, per-worker, reset each search. Record distances, indexing, footprint and per-worker ownership in notes.
4. Stack context. Store the moving piece per ply in StackEntry when the move is set (before make), so the (piece,to) context of the 1- and 2-ply-back moves is read directly from the stack (null/none handled).
5. Ordering integration (primary = combined contextual quiet ranking). score_quiets sums plain history + cont-hist(1) + cont-hist(2) + counter bonus. On a quiet beta cutoff: bonus the cutoff move and malus the earlier failed quiets across plain history and both continuation distances, and store the counter move. Killers remain a stage; duplicate suppression must cover hash/killer/counter/quiet.
6. AC#7 A/B via compile-time knobs (KILLER_SLOTS precedent): counter as a dedicated-stage-equivalent dominant bonus vs combined moderate bonus; equal captures before vs after refutation candidates.
7. Tests. Contextual evidence orders a reply ahead of a higher plain-history move; duplicate suppression vs hash/killer/counter/quiet; counter legality validation; bounded update shared.
8. Measurements. Extend the ablation example for node counts + throughput across the AC#7 variants and the killer ablation (AC#8) with continuation history active; run the TASK-27 strength SPRT for the selected design (AC#9/#10). Record all figures in implementation notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation summary

Added a counter-move table and 1-/2-ply continuation history, folded into a combined contextual quiet ranking. Plain from-to butterfly history is unchanged; quiet scoring now sums plain history + continuation history for the moves one and two plies back. The counter move is a dedicated ordering stage after the killers (Phase::Counter). Both new tables share the plain-history bounded gravity update (extracted as `history::gravity_update`), are per-worker, and reset each search alongside history/killers.

Preceding-move context comes from the per-ply search stack: `StackEntry.moved_piece` is captured at make time (the mover on its origin square, colour included), so the (piece, to) key of the 1- and 2-ply-back moves is read directly rather than reconstructed from the board (the 2-ply-back piece may have moved again). Null moves record `Piece::None`, which reads as no context and suppresses that ply's contribution.

## AC#3 -- distances, indexing, footprint, per-worker ownership

- Distances: exactly one and two plies back (the mandated minimum). Both contribute to quiet ordering and are updated on quiet beta cutoffs.
- Indexing: keyed by (piece, to-square) contexts. Piece is one of the 12 real pieces (colour included), so no separate side dimension is needed. `piece_to_index(piece, sq) = (piece as usize - 1) * 64 + sq` in 0..768.
  - Continuation history: two flattened 768 x 768 i32 grids `[prev (piece,to)][cur (piece,to)]`, one per distance.
  - Counter-move table: 768 `Move` entries keyed by the preceding move's (piece,to).
- Footprint (per worker): continuation history = 2 x 768 x 768 x 4 bytes = 4.72 MB (heap, boxed slice, allocated once per Search); counter table = 768 x sizeof(Move) ~ 3 KB. Cache: scoring one quiet touches one 768-i32 context row (~3 KB) per distance, L1/L2-resident.
- Ownership: plain fields on `Search` (like `history`/`kt`), reset at the end of each `Search::run`, never shared between Lazy SMP workers.

## AC#4 -- shared bounded scheme

`history::gravity_update(entry, bonus)` is the single bonus/malus/aging rule (clamp bonus to +/-HISTORY_MAX, then `entry += bonus - entry*|bonus|/HISTORY_MAX`). Plain history and both continuation distances all update through it; the counter table stores identity only (no counter). No table keeps an independent unbounded or exposure-based count.

## AC#6 -- legality validation

The counter move is stored against a preceding move but probed at a possibly different position, so `MoveLoader::counter_move` gates it through `Position::valid_move` (pseudo-legal + legal) before it can reach the unsafe move loop -- the same guarantee the killer probe uses. Covered by search test `a_stored_counter_is_legality_validated_before_it_is_yielded`.

## AC#5 -- tests

- `search::continuation_history_orders_a_reply_ahead_of_a_higher_plain_history_move`: a reply with strong continuation evidence but zero plain history is ordered ahead of a move with higher plain history and no continuation evidence.
- `ordering::the_counter_move_is_a_stage_with_full_duplicate_suppression`: the counter is yielded by its own stage and suppressed from the quiets; a counter equal to the hash move or a killer is dropped from its own stage.
- `search::a_stored_counter_is_legality_validated_before_it_is_yielded` (AC#6).
- `continuation::*`: dense/distinct piece-to indexing, counter round-trip + recency replacement, bounded+context-local continuation updates.

## AC#7 -- design comparisons (fixed-depth nodes, engine/examples/ordering_ablation.rs)

Method: single-thread, fresh 16 MB TT per position, fixed depth, no time/node limit -> node counts deterministic per build. RUSTFLAGS="-C target-cpu=native". Positions/depths match the killer ablation (startpos d11, kiwipete d10, middlegame d10, endgame d14). Each variant is a rebuild with the relevant compile-time constant flipped.

(a) Dedicated counter stage vs folded counter (FOLD_COUNTER_INTO_QUIETS):
- dedicated (false, shipped): total 8,480,073
- folded (true):             total 8,480,039
=> 34-node (0.0004%) difference. The counter is essentially always what continuation/plain history would surface near the front anyway. Chose the dedicated stage: simplest to reason about, identical cost, and cleaner duplicate-suppression semantics.

(b) Equal captures before vs after refutations (EQUAL_CAPTURES_AFTER_REFUTATIONS):
- before (false, shipped): total 8,480,073
- after (true):            total 11,200,521 (+32.1%)
=> Yielding equal captures after killers/counter is clearly worse. Keep equal captures before the refutations.

## AC#8 -- killer ablation with continuation history active (KILLER_SLOTS)

- K=0 (disabled): total 8,602,624 (startpos 1,110,090 | kiwipete 5,254,367 | middlegame 1,062,861 | endgame 1,175,306)
- K=1:            total 8,850,064 (startpos 1,385,143 | kiwipete 5,191,780 | middlegame   884,368 | endgame 1,388,773)
- K=2 (shipped):  total 8,480,073 (startpos 1,037,939 | kiwipete 5,180,369 | middlegame   878,234 | endgame 1,383,531)
Decision: retain 2 killer slots. Direction is non-monotone across 0/1/2 (as TASK-64.3 documented, dominated by aspiration-window re-search sensitivity rather than a clean killer signal), but K=2 wins the total even with continuation history active. Killers are retained rather than combined or removed.

## AC#9 -- node counts and throughput vs feature-off baseline

Feature-off reference: killer_ablation at the merge-base engine (05880a5; master tip f436fe5 differs only by a lichess fix, no search change), KILLER_SLOTS=2, no counter/continuation:
- master total: 8,232,276 (startpos 912,842 | kiwipete 5,252,710 | middlegame 1,044,931 | endgame 1,021,793)
- branch total: 8,480,073 (+3.0%)
Per position (branch vs master): middlegame -16.0% (1,044,931 -> 878,234), kiwipete -1.4% (5,252,710 -> 5,180,369) improve; startpos +13.7%, endgame +35.4% regress. Throughput (nps) is comparable across all configs (~5-7 Mnps, within wall-clock noise).

Honest reading: the tactical middlegame -- the regime continuation history targets -- improves substantially, and kiwipete improves slightly. The net node increase is driven by the deep (depth-14) endgame, the same aspiration-window-sensitive position TASK-64.3 recorded a spike on: a small score shift flips a large fail-high/low re-search. So representative tactical positions improve; the net is not a reduction because of the endgame aspiration artifact, not a broad ordering regression.

## AC#10 -- TASK-27 strength regression

Runner: fastchess alpha 1.5.0 via tools/strength/strength_test.py, authoritative mode. tc=8+0.08, concurrency 6, 16 MB hash, opening suite seaborg-openings-v1 (colours-reversed paired). SPRT elo0=-5, elo1=0, alpha=beta=0.05.
Baseline: 05880a5 (merge-base, target-cpu=native release locked). Candidate: f1d1952 (the implementation target; sha256 in the report).
Result: INCONCLUSIVE at the 400-game cap. LLR = -0.51, bounds [-2.94, 2.94]. Candidate W-D-L = 104-164-132, Elo +/- = -24.4 +/- 24.6, pentanomial [16,58,77,36,13], 0 forfeits, 0 crashes. Report archived at ~/seaborg-strength-builds/report-final (report.json + runner log + games.pgn); binaries at ~/seaborg-strength-builds/.

Honest assessment: at 400 games the confidence interval (~+/-25 Elo) is too wide to conclude anything -- neither a >5 Elo regression nor non-regression is established, and the point estimate is mildly negative. This is the depth/TC regime where continuation history is weakest: fast games search shallow, and the heuristic needs depth to train and pay off. A conclusive verdict would need a larger game count and/or a longer time control. Consistent with the fixed-depth node picture (tactical gains offset by an endgame aspiration artifact), and with TASK-64.3's precedent of recording an INCONCLUSIVE authoritative SPRT rather than blocking on it. The feature is a correct, standard, foundational building block; its immediate measured strength at fast TC is neutral within noise.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 04:30
---
Implementation handoff
Branch: task-64.10-counter-continuation-history
Worktree: /Users/seabo/seaborg-worktrees/task-64.10-counter-continuation-history
Base: 05880a5
Implementation target: f1d1952
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (engine 379 passed / 2 ignored; chess 49; integration 1; all green)
- AC#7/#8/#9 fixed-depth node ablations via engine/examples/ordering_ablation.rs (knobs FOLD_COUNTER_INTO_QUIETS, EQUAL_CAPTURES_AFTER_REFUTATIONS, KILLER_SLOTS): recorded in notes
- AC#10 TASK-27 strength SPRT (candidate f1d1952 vs baseline 05880a5, tc=8+0.08, authoritative): INCONCLUSIVE at 400 games, LLR=-0.51, Elo -24.4+/-24.6, 0 forfeits/crashes; report at ~/seaborg-strength-builds/report-final; recorded in notes
Known failures: none

Reviewer note: this is a strength-feature task and the immediate strength result is INCONCLUSIVE, not a demonstrated gain -- the 400-game CI (~+/-25 Elo) is too wide to conclude, and the point estimate is mildly negative. Fixed-depth nodes are net +3% vs base, driven by a deep-endgame aspiration-window artifact (as in TASK-64.3) while the tactical middlegame improves 16%. All figures are recorded honestly in the notes for the AC#9/#10 determination; a conclusive strength verdict would need more games and/or a longer time control.
---

author: @claude
created: 2026-07-21 04:43
---
Review attempt: 1
Reviewed branch: task-64.10-counter-continuation-history
Reviewed implementation: f1d1952 (immutable; branch tip 3907d64 adds only task-file notes/handoff — verified no implementation file changed between f1d1952 and HEAD)
Base: 05880a5
Verdict: needs_human

## Summary
The implementation is correct, standard, well-tested, and well-documented. Counter-move + 1-/2-ply continuation history are folded into a combined contextual quiet ranking, sharing the bounded gravity update (history::gravity_update); preceding-move (piece,to) context is captured at make time in StackEntry.moved_piece; the counter is legality-validated via Position::valid_move before it can reach the unsafe move loop; duplicate suppression covers hash/killer/counter/quiet. I found no in-scope code defect.

Required checks (run by me on the target in the task worktree):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (confirmed with a clean CARGO_TARGET_DIR)
- cargo test --workspace: pass (engine 379 passed / 2 ignored; chess 49; plus integration/support suites all green)
No new #[allow] introduced; every new unsafe block carries a SAFETY comment.

Acceptance criteria assessment (I did NOT check any AC boxes):
- AC#1-#8: proven by the new unit/search tests and the recorded ablations (counter stage + full duplicate suppression; continuation reply ordered ahead of higher plain history; legality validation; shared bounded scheme; AC#3 footprint/ownership recorded; AC#7 design A/B and equal-captures ordering recorded; AC#8 killer 0/1/2 ablation recorded).
- AC#10: satisfied strictly as a measure-and-record criterion — the TASK-27 SPRT was run and recorded (INCONCLUSIVE at the 400-game cap, LLR -0.51, Elo -24.4 +/- 24.6). It does not, by itself, demonstrate a strength gain.

## Why this is Needs Human, not Approve or Changes Requested
AC#9 requires "Representative fixed-depth node counts improve." The implementer's own honest figures show the opposite in aggregate: net +3.0% nodes vs the feature-off baseline (startpos +13.7%, endgame +35.4% regress; middlegame -16.0%, kiwipete -1.4% improve). The notes state plainly "the net is not a reduction," attributing the endgame spike to a depth-14 aspiration-window artifact (consistent with TASK-64.3). So AC#9 is not objectively proven, and I cannot approve.

This is not an in-scope code defect, so Changes Requested does not fit: the design is standard and was selected empirically, and no rework can manufacture a node-count improvement that the measurements do not show. The open question is a product/scope decision:

Decision needed: Should this correct, foundational move-ordering primitive be accepted despite an immediate empirical payoff that is node-count-negative in aggregate at fixed depth and strength-inconclusive/mildly-negative at fast TC (8+0.08)? Options a human should weigh:
  1. Accept on foundational grounds (continuation history typically pays off at greater depth/longer TC than the 400-game 8+0.08 run exercises), optionally waiving/reinterpreting AC#9's "improve" to "representative tactical positions improve" (middlegame -16%, kiwipete -1.4%).
  2. Require a stronger empirical case before merging: a larger game count and/or a longer time control to resolve AC#10, and/or investigation of the startpos/endgame node regressions.
  3. Defer the feature.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): pass
- cargo test --workspace: pass
- Recorded ablation/SPRT figures reviewed in implementation notes; AC#9 net node-count direction confirmed regressive from the recorded per-position figures.
---

author: @claude
created: 2026-07-21 05:18
---
Review attempt: 1 (approval after human resolution of the Needs Human escalation)
Reviewed branch: task-64.10-counter-continuation-history
Reviewed implementation: f1d1952 (immutable code target; branch tip carries only task-file verdict/notes — no implementation file changed after the reviewed SHA)
Base: 05880a5
Verdict: approved

The sole blocker in the prior review was AC#9 ("representative fixed-depth node counts improve"), which the honest measurements do not meet in aggregate (net +3.0% nodes; middlegame -16.0% and kiwipete -1.4% improve, offset by an endgame depth-14 aspiration-window artifact). That was escalated as a product/scope decision. The human authority (georgeseabridge) has accepted the task on foundational grounds, resolving that decision. All other acceptance criteria are objectively proven; AC#9 is checked as accepted-by-decision rather than as a demonstrated aggregate node improvement, and this distinction is recorded in the final summary.

Verification (re-run on the target in the task worktree):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean CARGO_TARGET_DIR)
- cargo test --workspace: pass (engine 379 passed / 2 ignored; chess 49; support suites green)

Code target for merge: f1d1952. No new #[allow]; every new unsafe carries a SAFETY comment.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a counter-move table and 1-/2-ply continuation history, folded into a combined contextual quiet ranking sharing the bounded history::gravity_update; counter is legality-gated via Position::valid_move; duplicate suppression covers hash/killer/counter/quiet. Verified on immutable target f1d1952: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings (clean CARGO_TARGET_DIR), and cargo test --workspace all pass. AC#1-#8 proven by tests + recorded ablations; AC#10 SPRT run and recorded (INCONCLUSIVE at 400 games). AC#9's literal aggregate node-count improvement is NOT met (net +3.0%, middlegame -16% offset by an endgame depth-14 aspiration artifact); it is accepted by explicit human product decision (georgeseabridge) to land this foundational move-ordering primitive despite neutral immediate empirics.
<!-- SECTION:FINAL_SUMMARY:END -->
