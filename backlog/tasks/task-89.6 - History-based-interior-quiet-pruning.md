---
id: TASK-89.6
title: History-based interior quiet pruning
status: Done
assignee:
  - '@codex'
created_date: '2026-07-29 13:47'
updated_date: '2026-07-29 18:45'
labels:
  - search
  - selectivity
  - strength
dependencies: []
references:
  - engine/src/search.rs
  - tools/diag/selectivity_profile.py
  - BENCHMARKS.md
parent_task_id: TASK-89
type: feature
ordinal: 160000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Continuation of the TASK-88/89 selectivity programme. A re-baseline of the TASK-82/88 profile on current master (recorded in BENCHMARKS.md) shows the strength gap versus a frontier engine is now entirely selectivity, not speed: single-thread NPS is at parity, but geomean EBF is ~2.30 vs ~2.01 and effective depth is ~17 vs ~23 plies at 1500ms on bench-positions.epd - a ~6-ply deficit that is fully accounted for by the EBF gap.

A new per-remaining-depth width profile (selstats) localises the excess width. Move-count pruning (LMP, TASK-64.8) is confined to remaining depth <= 3; the instant it switches off at remaining depth 4 the searched quiet fan-out triples (from ~2 to ~6 quiets per node) and peaks near 10.5 quiets per node around remaining depth 6. The tree interior (remaining depth 4-8) is searched at near-full quiet width.

A new shadow-counter screen (selstats) ranks candidate rules for pruning that interior tail by coverage (fraction of searched quiet-phase moves removed) versus damage (fraction of the quiets that actually raised alpha or forced the cutoff that the rule would wrongly kill). History-based pruning - prune a history-ordered quiet with negative combined history in the interior - is decisively the best lever: ~42% coverage at ~5.7% damage (~7.4 coverage-per-damage), versus ~4.4 for the best move-count schedule. Move-count schedules prune blindly by position; history prunes by move quality, which is why it dominates. Killers, counter-move and the TT move are separate ordering phases and are already exempt.

This task adds history-based quiet pruning to the main-search interior and measures it. The width and shadow-counter instruments were built on branch diag-phase1-depth-width (commits 505adc9 and 9ac84b1) and must be available (landed or reproduced) so AC#2 can be verified from our own profile.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 History-based pruning of interior quiet moves is added to the main search, gated by a toggle like the other forward-pruning steps, and measured by a self-play SPRT at tc=10+0.1 versus the pre-change baseline and recorded in BENCHMARKS.md
- [x] #2 The selstats profile confirms the intended signal: the interior (remaining depth 4-8) quiet width and the geomean EBF both drop, without the LMR re-search rate or non-PV fail-low behaviour degrading pathologically
- [x] #3 Retained only on a non-negative SPRT; a negative result is recorded and reverted
- [x] #4 Conclusion is derived from Seaborg own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add interior history-based quiet pruning to the main search move loop (search.rs), mirroring the winning shadow-counter rule C3 from TASK-91: prune a quiet-phase move whose combined main+continuation history is negative, once past a short move-count prefix, in the tree interior (remaining depth <= 8). Guards match the existing forward-pruning steps: non-PV, not in check, quiet phase only (killers/counter/TT are separate phases, already exempt), and a checking move is exempt.
2. Place the prune after LMP and futility, before the selstats width/shadow recording, so the population it acts on matches the shadow measurement and the selstats width profile reflects the reduction.
3. Add constants HISTORY_PRUNING_MAX_DEPTH (8) and HISTORY_PRUNING_MOVE_THRESHOLD (3) with reader-standalone doc comments, and a #[cfg(test)] history_pruning_disabled toggle + history_pruning_enabled() helper matching the LMP/LMR pattern.
4. Add regression tests: prune shrinks the tree at fixed depth; prune keeps a decisive capture (never reaches the winning prefix).
5. Run required checks (fmt, clippy -D warnings, test --workspace).
6. Measure: selstats before/after (interior quiet width + geomean EBF) and a self-play SPRT at tc=10+0.1 vs the pre-change baseline; record in BENCHMARKS.md. Retain only on a non-negative SPRT; otherwise record and revert.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Selstats (AC#2) confirms the mechanism decisively. Fixed depth 14 on bench-positions.epd: interior quiet width more than halves — rem-depth 4 6.15->2.76, 5 9.51->4.14, 6 10.53->4.45, 7 7.13->3.31, 8 7.44->3.29 quiets/node; depth>8 unchanged (prune is capped at 8). Geomean EBF 2.710->2.619; nodes-to-depth-14 drop 20.36M->14.59M (-28%). Fixed 2M nodes/pos: mean depth reached 16.25->17.30 (+1.05 ply), EBF 2.472->2.370. LMR re-search rate rises 1.75%->2.68% (fixed depth) / 2.73%->3.58% (fixed nodes); non-PV fail-low 23.0%->26.3% / 25.1%->28.5% — both modest, not pathological.

Regression surfaced by gives_correct_answers: the depth-24 KPvKP pawn-race (8/6pk/8/8/8/8/P7/K7 w) drops from a won >=+450 to cp 0. The candidate plays a2a4 and reads the win as a draw; interior history pruning removes the winning quiet king step-aside (a1b1/a1b2) and follow-up king maneuvers deep in the sparse endgame, where history is unreliable and maneuvering moves carry negative scores. Baseline finds a1b1 (+475 at depth 20, climbing past +1300). This is a genuine soundness cost of the flat hist<0 interior rule, and the reason AC#3 gates retention on a non-negative SPRT. Running the SPRT now as the arbiter.

PARKED: Needs Human. Two coupled reasons.

(1) Genuine soundness regression fails a required check. cargo test --workspace is red on search::tests::gives_correct_answers: the depth-24 KPvKP pawn-race (8/6pk/8/8/8/8/P7/K7 w) is a won >=+450 ending that the candidate reads as cp 0 and plays a2a4, because interior history pruning removes the winning quiet king step-aside (a1b1/a1b2) and its follow-up maneuvers deep in the sparse endgame, where history is unreliable and maneuvering moves carry negative scores. Baseline finds a1b1 (+475 at depth 20, climbing past +1300). This is a real defect, not an over-specified bound, so widening the test to accept a drawn score for a won ending would mask it and is not an acceptable fix.

(2) The mandated retention SPRT (AC#1, AC#3) cannot be run trustworthily now. The shared measurement host (Apple M3 Pro, 12 cores) is saturated at load ~11 by another active session's open-ended node-budget gauntlet sweep in worktree task-92-unbalanced-opening-book. Stacking a concurrency-6 tc=10+0.1 SPRT on top oversubscribes to ~18 and causes time forfeits; BENCHMARKS.md and repo practice require an idle host and never trust contended time-based results. My launched SPRT was stopped to avoid biasing both runs.

Decisions needed from a human:
- (a) Provide/free a quiet measurement host (or authorize a scheduled quiet-time run) so the retention SPRT can run against the committed target 1dcb1eb: python3 tools/strength/strength_test.py --baseline <b46e8bb-release> --candidate <1dcb1eb-release> --limit tc=10+0.1 --concurrency 6 --hash-mb 64 --mode authoritative (elo0=-5 elo1=0 alpha=beta=0.05). Native release binaries staged at /tmp/sb-base-89.6 and /tmp/sb-cand-89.6.
- (b) Decide whether the KP-endgame soundness cost should be fixed with a guard (e.g. skip the prune in pawn-only endgames / require non-pawn material) BEFORE measuring — which would change the rule from the measured shadow C3 and need re-profiling — or whether to revert per AC#3 if the SPRT is negative. Prior selectivity experiments (BENCHMARKS.md experiments 2 and 3) moved the profile correctly yet did not convert to Elo, so a negative/neutral SPRT is a live possibility.

State: implementation + tests + selstats evidence committed on task-89.6-history-interior-quiet-pruning at 1dcb1eb (base b46e8bb). fmt and clippy -D warnings are green; only the endgame behaviour test is red. Worktree clean. Not moved to In Review because a required check is red and the retention gate is unmeasured.

UNBLOCKED: rig (24-core, idle) available for SPRTs. Investigated the soundness fix per the two approved directions.

#2 depth-scaled history margin: DEAD END (data-driven). The winning maneuvering move and the junk quiet tail both sit at only mildly-negative history, so no magnitude threshold separates them. A margin large enough to save the KPvKP win (K=2048/depth) prunes essentially nothing on bench-positions.epd (profile byte-identical to baseline); any smaller margin re-breaks the win. Reverted.

#3 quiet-mobility floor: THE FIX. The failure is low-mobility, not history-magnitude. A lone king has <=8 destinations, so requiring a minimum quiet count before pruning exempts the whole king-and-pawn class while barely touching the middlegame interior (15-40 quiets/node). Added OrderedMoves::quiet_count() and HISTORY_PRUNING_MIN_QUIETS=12. Results: KPvKP win now tracks the un-pruned search from depth 19 (was collapsed to a draw); floor=12 keeps ~90% of the flat rule's EBF reduction (fixed-depth-14 EBF 2.710->2.628 vs flat 2.619) and +0.9 of the +1.05 ply at fixed 2M nodes. Committed b6bbf5d. fmt/clippy/test green; added a theory-agnostic endgame regression test (pruning must never turn the un-pruned search's won ending into a draw). The one lichess concurrency test that failed under concurrent load is pre-existing flaky (passes 3/3 in isolation).

SPRT #1 (upper bound, UNSOUND flat 1dcb1eb vs base, rig, tc=10+0.1, c=22): candidate 289-595-423, Elo -35.7 [-54.8,-16.9] over 1307 games. Decisively negative; stopped early (conclusive as a diagnostic). Open question it leaves: is the loss the mechanism or the soundness losses the floor fixes?

SPRT #2 (RETENTION, SOUND floor-12 b6bbf5d vs base, same config) LAUNCHED on rig, running. This decides keep (AC#3) vs revert.

RETENTION SPRT (sound floor-12 b6bbf5d vs base b46e8bb): AUTHORITATIVE FAIL. LLR -2.97 (bounds +/-2.94), Elo -31.97 [-48.5,-15.6], W-D-L 401-772-560, 1733 games (rig 24-core, tc=10+0.1, c=22, hash 64, elo0=-5 elo1=0 alpha=beta=0.05, openings-v1.epd, native release). The unsound flat upper bound was ~-35.7 over 1307 games — fixing the endgame soundness bug changed strength by nothing, so the loss is the mechanism (a no-re-search discard of interior subtrees that were being spent on real alternatives), not the endgame casualties.

AC#3 applied: reverted. engine/ restored to base b46e8bb exactly (git diff b46e8bb -- engine/ is empty). The full write-up is kept in BENCHMARKS.md; the KP soundness regression, the margin dead-end, the mobility-floor fix, and both SPRTs are recorded there as an informative negative.

Implementation handoff
Branch: task-89.6-history-interior-quiet-pruning
Worktree: /Users/seabo/seaborg-worktrees/task-89.6-history-interior-quiet-pruning
Base: b46e8bb
Implementation target: b5450fa
Nature: informative-negative — engine code reverted to base; net change is BENCHMARKS.md write-up + this task record. Review should verify (1) the measurement is sound and own-signal-only, (2) the revert is clean (engine == base), (3) the BENCHMARKS.md write-up is accurate.
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (476 engine + 161 lichess + others; 0 failed, 2 ignored)
- git diff b46e8bb -- engine/: empty (clean revert to base behaviour)
Known failures: none. (During investigation the lichess concurrency test run::tests::incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked failed once under concurrent load; it is pre-existing flaky and passes in isolation and in the final clean run.)

CLOSING SUMMARY (why this is Done as an informative negative).

What we set out to do: add the shadow-counter's top-ranked selectivity lever — history-based pruning of the interior quiet tail — and measure whether it converts to strength.

What happened:
- Mechanism confirmed (AC#2). selstats showed exactly the intended movement: interior (rem-depth 4-8) quiet width more than halved, geomean EBF 2.71->2.62 (fixed depth 14), nodes-to-depth-14 -28%, +1.05 ply at fixed 2M nodes, with LMR re-search and non-PV fail-low rising only modestly.
- Soundness regression found and fixed. The flat hist<0 rule turned a won KPvKP ending into a draw. A depth-scaled history margin could not separate the winning maneuvering move from the junk tail (both only mildly negative) and was a dead end; a quiet-mobility floor (>=12 quiets, since a lone king has <=8 moves) fixed it cleanly while retaining ~90% of the EBF gain.
- Retention decided by SPRT (AC#1/AC#3). Sound floor-12 variant vs base on the rig (tc=10+0.1): AUTHORITATIVE FAIL, LLR -2.97, Elo -31.97 [-48.5,-15.6], W-D-L 401-772-560, 1733 games. The unsound flat upper bound was ~-35.7 over 1307 games — so fixing the endgame casualties changed strength by nothing, proving the loss is the mechanism (a no-re-search discard of interior subtrees that were being spent on real alternatives), not the soundness bug.

Why closed: AC#3 mandates revert on a negative SPRT. The engine is restored to base b46e8bb exactly (git diff b46e8bb -- engine/ is empty); the full write-up is retained in BENCHMARKS.md as an informative negative, alongside the methodological lesson (coverage-per-damage ranks *which* quiets a rule removes, not whether removing them helps — a screen, not a strength predictor). All four acceptance criteria are satisfied by the measurement-and-revert; there is no engine change to ship. Disposition: reverted, recorded, Done.
<!-- SECTION:NOTES:END -->
