---
id: TASK-91
title: >-
  Extend the selectivity profile: per-remaining-depth width and shadow-prune
  ranking
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-29 13:57'
updated_date: '2026-07-29 14:14'
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
- [x] #1 A per-remaining-depth tree-width profile is added to the selstats instrumentation and surfaced by selectivity_profile.py: nodes, mean moves per node, and mean quiets per node by remaining depth over the non-PV not-in-check population
- [x] #2 Shadow-counters score candidate quiet-pruning rules by coverage and damage without altering the search, and selectivity_profile.py reports the ranking
- [x] #3 The instrumentation is behaviour-transparent: a selstats build searches the same tree as a default build (identical EBF, quiescence fraction, and first-move-cutoff), and the counters are compiled out of non-selstats builds
- [x] #4 Repository-required checks pass: cargo fmt --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace
- [x] #5 BENCHMARKS.md records the re-baseline (NPS/EBF/depth), the per-depth width cliff, and the shadow-counter ranking
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Formalises diagnostic work already committed on this branch (505adc9 width profile, 9ac84b1 shadow-counters, dd47502 BENCHMARKS record + follow-up TASK-89.6 file). No code was changed while formalising.

Delivered: per-remaining-depth width profile and shadow-counter ranking in the selstats instrumentation (engine/src/trace.rs, engine/src/search.rs), parsed and printed by tools/diag/selectivity_profile.py; BENCHMARKS.md "Selectivity re-baseline and interior-width profile (2026-07-29)" section.

Behaviour-transparency evidence (AC#3): fixed-depth-14 profile over bench-positions.epd is byte-for-byte identical on the pre-change and post-change selstats builds on every metric that reflects the tree shape - EBF 2.710, quiescence 53.3%, first-move-cutoff 88.5% - confirming the added counters observe decisions without changing them. All new fields/increments sit behind #[cfg(feature = "selstats")] and are absent from default builds.

Note for the reviewer: the follow-up behaviour-change task TASK-89.6 (history-based interior quiet pruning) was created before this task and its file rides on this branch (commit dd47502); it is intentional context, not part of this task diff.

Implementation handoff
Branch: diag-phase1-depth-width
Worktree: /Users/seabo/seaborg-worktrees/diag-phase1-depth-width
Base: 90f1dea
Implementation target: dd47502
Resolved findings: none
Verification:
- cargo fmt --check: pass (exit 0)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0)
- cargo test --workspace: pass (722 passed, 0 failed)
Known failures: none
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-29 14:14
---
## Review verdict: APPROVED — Ready to Merge

**Code target (immutable):** dd47502
**Base:** 90f1dea
**Branch:** diag-phase1-depth-width
**Reviewed:** full base..target diff (engine/src/trace.rs, engine/src/search.rs, tools/diag/selectivity_profile.py, BENCHMARKS.md, plus the sanctioned TASK-89.6 follow-up file).

### Acceptance criteria
- **AC#1 (per-depth width profile):** PASS — ran the selstats build over bench-positions.epd; the tool prints nodes / mean moves-per-node / mean quiets-per-node bucketed by remaining depth over the non-PV not-in-check population, with the width cliff at remaining depth 4 visible (quiets/node ~2 → ~5-7).
- **AC#2 (shadow-counter ranking):** PASS — the same run prints the coverage/damage ranking for the four candidate rules; hist<0,d<=8 ranks best on coverage-per-damage, matching the BENCHMARKS write-up. Labels/order in the Python match shadow_prune_mask.
- **AC#3 (behaviour-transparent + compiled out):** PASS — every counter is a post-decision increment reading already-computed values (depth, move_count, is_quiet, lmr_history, phase, Node::pv, node_in_check); none mutates search state. SelStats, the Tracer.sel field, all recorders, and SEL_DEPTH_BUCKETS/SHADOW_CANDIDATES are entirely under #[cfg(feature="selstats")]; the default cargo test build compiles and passes without them. shadow_good ⊆ shadow_denom and quiet_good_total accounting are consistent (good move always passes the earlier searched-move record within the same iteration; move_count/lmr_history unchanged across the iteration).
- **AC#4 (required checks):** PASS — cargo fmt --check (exit 0); cargo clippy --workspace --all-targets --all-features -- -D warnings on a clean CARGO_TARGET_DIR (exit 0, exercises selstats); cargo test --workspace (exit 0).
- **AC#5 (BENCHMARKS record):** PASS — 'Selectivity re-baseline and interior-width profile (2026-07-29)' documents Phase 0 (NPS parity / EBF / depth), Phase 1 (width cliff at the LMP boundary; LMP_MAX_DEPTH=3 confirmed in source), Phase 2 (shadow ranking).

### Other checks
- No new #[allow] directives. Comments are interpretable in place; 'Phase 2' references are accompanied by the actual reason.
- All additions are cfg-gated, so shipped/release and default builds are unchanged — perft/movegen benches are byte-identical; no benchmark run required.
- Post-target commits (3d0080a, 15d73c9) touch only the task file; no implementation change between dd47502 and the branch tip.

No blocking findings. Verification commands: cargo fmt --check; CARGO_TARGET_DIR=<clean> cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace; python3 tools/diag/selectivity_profile.py --seaborg <selstats-build> --suite bench-positions.epd.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Approved. Adds two off-by-default selstats diagnostic views: (1) a per-remaining-depth tree-width profile (nodes, mean moves/node, mean quiets/node over the non-PV not-in-check LMP-eligible population) in engine/src/trace.rs + engine/src/search.rs, and (2) shadow-counters that score four candidate quiet-pruning rules by coverage/damage without acting on the search. Both surfaced by tools/diag/selectivity_profile.py; BENCHMARKS.md records the re-baseline (Phase 0 NPS/EBF/depth), the rem-depth-4 width cliff (Phase 1), and the shadow ranking (Phase 2). Verified independently on immutable target dd47502: ran the selstats build over bench-positions.epd — the per-depth width table and coverage/damage ranking both render and the width cliff at remaining depth 4 is visible (AC#1, AC#2). Behaviour-transparency (AC#3) confirmed structurally: every counter is a post-decision increment reading already-computed values (no search-state mutation), and SelStats, its Tracer field, all recorders, and the two consts are entirely under #[cfg(feature="selstats")]; the default cargo test build compiles and passes without them. AC#4: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings pass on a clean CARGO_TARGET_DIR (exercising the selstats path); cargo test --workspace pass. AC#5: BENCHMARKS.md section present. No new #[allow]s; all additions cfg-gated so shipped hot path is unchanged (no perft/movegen regression). TASK-89.6 rides along as sanctioned follow-up context, not part of this diff.
<!-- SECTION:FINAL_SUMMARY:END -->
