---
id: TASK-89.6
title: History-based interior quiet pruning
status: To Do
assignee: []
created_date: '2026-07-29 13:47'
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
