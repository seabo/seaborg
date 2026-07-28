---
id: TASK-89.3
title: Deepen LMR for low-history and non-improving quiets
status: Done
assignee:
  - '@george'
created_date: '2026-07-27 15:08'
updated_date: '2026-07-28 22:31'
labels:
  - search
  - selectivity
  - strength
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-89
priority: medium
type: feature
ordinal: 154000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 3. Mechanism: the reduction distribution has almost no mass beyond 3 ply; widen the history-and-improving modulation so the least-promising late quiets are cut harder while the trusted prefix keeps its depth. Signal: the reduction-ply distribution shifts right without the re-search rate exploding.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The history/improving reduction modulation is widened; the change is measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [x] #2 The selstats profile confirms the reduction-ply distribution shifts right (more mass beyond 3 ply) without the re-search rate exploding
- [x] #3 Retained only on a non-negative SPRT; a negative is recorded and reverted
- [x] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Widen the LMR history+improving modulation in engine/src/search.rs behind the existing LMR_HISTORY_MODULATION / LMR_IMPROVING_MODULATION toggles. Split the symmetric history clamp into an asymmetric pair: LMR_HISTORY_DEEPEN_MAX (negative-history side, widened) vs LMR_HISTORY_EASE_MAX (positive side, unchanged so the trusted prefix keeps its depth). Lower LMR_HISTORY_DIVISOR to steepen the slope, and make the non-improving penalty a named constant that can be widened past 1 ply.
2. Build --features selstats binaries for a small candidate ladder and profile each with tools/diag/selectivity_profile.py (fixed depth 14 + fixed 2M nodes, bench-positions.epd, 64MB hash). Select the moderate point whose LMR reduction-ply distribution shifts right (more mass >=4 ply) without the re-search rate exploding; record the movement. Profile is selection only; SPRT is the arbiter.
3. Set the constants to the selected candidate; run cargo fmt --check, clippy -D warnings, cargo test --workspace.
4. Run self-play SPRT at tc=10+0.1 (elo0=-5, elo1=0) baseline (branch base) vs candidate via tools/strength/strength_test.py.
5. Record profile movement + SPRT verdict in BENCHMARKS.md. Retain on non-negative SPRT; on a negative, revert the constants to baseline and record the informative negative (AC#3). No cross-engine diffing (AC#4).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation complete (commit 308c85b). Widened LMR history+improving modulation: asymmetric history clamp (LMR_HISTORY_DEEPEN_MAX 3*PLY vs LMR_HISTORY_EASE_MAX 2*PLY), LMR_HISTORY_DIVISOR 40->30, LMR_IMPROVING_PENALTY 1.5*PLY.

Selection sweep (selstats, bench-positions.epd, 64MB; base=master-equiv refactor, candidates A/B/C):
 fixed-depth 14 reduction-ply mass >=4ply | mean red | re-search | EBF | fixed-2M-nodes depth
 base 15.8% | 2.45 | 1.75% | 2.710 | 16.25
 A    16.5% | 2.49 | 1.66% | 2.701 | 16.40
 B    17.2% | 2.51 | 1.73% | 2.669 | 16.90  <- selected (moderate; best fixed-node depth)
 C    17.6% | 2.55 | 1.67% | 2.650 | 16.45
Distribution shifts right monotonically with re-search flat (mechanism confirmed, AC#2). B chosen as the moderate point with the largest fixed-node depth gain (+0.65 ply). SPRT is the arbiter.

Two fixed-depth surfacing tests deferred one iteration by the harder reduction (objective best move unchanged, verified with the hand-crafted eval the tests use): KP-race win surfaces depth 25 not 24 (best a1b2/a1b1 already at d24); mate-parity mate at depth 9 not 8 (best b5e2 stable). Bumped both depths + comments.

Required checks: fmt PASS, clippy -D warnings PASS, cargo test --workspace PASS (0 failures).

SPRT launched tc=10+0.1, elo0=-5 elo1=0, alpha=beta=0.05, 64MB, concurrency 6. baseline git:e35205c (sha 5a110dc1) vs candidate git:308c85b (sha de766c9b). Pending.

OUTCOME: informative negative, closed early by human decision (not worth further effort now).

SPRT tc=10+0.1 (elo0=-5, elo1=0, alpha=beta=0.05), baseline git:e35205c vs candidate git:308c85b, stopped at 2319 games (no boundary crossed): cand W-D-L 545-1202-572, score 49.42%, Elo ~= -4.0 +/- 7.2 (trinomial). No improvement; leaning slightly negative, CI spans zero.

Same shape as TASK-89.2: the selstats profile moved the intended signal (reduction-ply distribution shifted right, re-search rate stayed flat ~1.7%, +0.65 ply at fixed nodes) but it did not convert to strength. The distrusted-tail depth reclaimed at equal nodes is a shallower search of real alternatives, not free selectivity. Not pursued further; the change is not merged (master unchanged). LMR can be revisited once the rest of the engine gives it more raw material.
<!-- SECTION:NOTES:END -->
