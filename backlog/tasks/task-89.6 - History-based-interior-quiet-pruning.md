---
id: TASK-89.6
title: History-based interior quiet pruning
status: Needs Human
assignee:
  - '@codex'
created_date: '2026-07-29 13:47'
updated_date: '2026-07-29 15:20'
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
- [ ] #1 History-based pruning of interior quiet moves is added to the main search, gated by a toggle like the other forward-pruning steps, and measured by a self-play SPRT at tc=10+0.1 versus the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms the intended signal: the interior (remaining depth 4-8) quiet width and the geomean EBF both drop, without the LMR re-search rate or non-PV fail-low behaviour degrading pathologically
- [ ] #3 Retained only on a non-negative SPRT; a negative result is recorded and reverted
- [ ] #4 Conclusion is derived from Seaborg own signals and self-play; no cross-engine behaviour diffing
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
<!-- SECTION:NOTES:END -->
