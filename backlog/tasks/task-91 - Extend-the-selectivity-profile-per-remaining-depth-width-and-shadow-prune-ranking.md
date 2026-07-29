---
id: TASK-91
title: >-
  Extend the selectivity profile: per-remaining-depth width and shadow-prune
  ranking
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-29 13:57'
updated_date: '2026-07-29 13:57'
labels:
  - search
  - selectivity
  - diagnostics
dependencies: []
references:
  - engine/src/trace.rs
  - engine/src/search.rs
  - tools/diag/selectivity_profile.py
  - BENCHMARKS.md
type: feature
ordinal: 161000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Diagnostic tooling, a companion to the TASK-88 selectivity profile. Extends the off-by-default selstats search instrumentation with two views that localise Seaborg selectivity deficit and rank the fix, plus the BENCHMARKS.md record of the re-baseline that motivated them.

(1) Per-remaining-depth tree-width profile: over the LMP-eligible node population (non-PV, not in check), count nodes, moves recursed, and quiet moves recursed, bucketed by remaining depth. Surfaced by tools/diag/selectivity_profile.py as a moves/quiets-per-node-by-depth table. It shows the tree-width cliff at the move-count-pruning boundary.

(2) Shadow-counters: score a small set of candidate quiet-pruning rules without acting on the search, by coverage (fraction of searched quiet-phase moves the rule would remove) and damage (fraction of the quiets that actually raised alpha or forced the cutoff that the rule would wrongly kill). Surfaced by the same script as a coverage/damage ranking. It ranks the candidate levers before any behaviour change or SPRT.

Both are behaviour-transparent: every counter is written after the decision it observes, so a selstats build searches the same tree as a default build, and the fields and increments are compiled out of shipped builds. The findings and the motivating re-baseline (NPS now at parity with Stockfish 18; the strength gap is entirely selectivity) are written up in BENCHMARKS.md. The follow-up behaviour change these instruments point to is tracked separately as TASK-89.6.

Implementation is already complete on branch diag-phase1-depth-width (commits 505adc9, 9ac84b1, dd47502 on base master 90f1dea).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A per-remaining-depth tree-width profile is added to the selstats instrumentation and surfaced by selectivity_profile.py: nodes, mean moves per node, and mean quiets per node by remaining depth over the non-PV not-in-check population
- [ ] #2 Shadow-counters score candidate quiet-pruning rules by coverage and damage without altering the search, and selectivity_profile.py reports the ranking
- [ ] #3 The instrumentation is behaviour-transparent: a selstats build searches the same tree as a default build (identical EBF, quiescence fraction, and first-move-cutoff), and the counters are compiled out of non-selstats builds
- [ ] #4 Repository-required checks pass: cargo fmt --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace
- [ ] #5 BENCHMARKS.md records the re-baseline (NPS/EBF/depth), the per-depth width cliff, and the shadow-counter ranking
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a per-remaining-depth width profile to SelStats in engine/src/trace.rs: depth_nodes/depth_moves/depth_quiets arrays bucketed by remaining depth, plus sel_depth_node and sel_move_searched recorders; emit the arrays in sel_json.
2. Record the width profile from the main-search move loop in engine/src/search.rs: sel_move_searched at the post-futility point where a move is committed to search, sel_depth_node once per fully-searched node, both gated to the non-PV not-in-check population.
3. Add shadow-counters: shadow_prune_mask (four candidate rules), shadow_denom/shadow_pruned/quiet_good_total/shadow_good in SelStats, sel_shadow_searched and sel_shadow_good recorders; call them at the same searched-move point and at the alpha-raise/cutoff point.
4. Parse and print both views in tools/diag/selectivity_profile.py (depth-width table + coverage/damage ranking).
5. Record the Phase 0-2 re-baseline, the width cliff, and the shadow ranking in BENCHMARKS.md.
6. Verify behaviour-transparency (identical EBF/quiescence/first-move-cutoff vs the pre-change profile) and run the repo-required checks.

Note: implementation already committed on this branch (505ac9..dd47502); this task formalises it for independent review.
<!-- SECTION:PLAN:END -->
