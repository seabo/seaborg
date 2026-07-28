---
id: TASK-89.3
title: Deepen LMR for low-history and non-improving quiets
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-27 15:08'
updated_date: '2026-07-28 17:00'
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
- [ ] #1 The history/improving reduction modulation is widened; the change is measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [ ] #2 The selstats profile confirms the reduction-ply distribution shifts right (more mass beyond 3 ply) without the re-search rate exploding
- [ ] #3 Retained only on a non-negative SPRT; a negative is recorded and reverted
- [ ] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Widen the LMR history+improving modulation in engine/src/search.rs behind the existing LMR_HISTORY_MODULATION / LMR_IMPROVING_MODULATION toggles. Split the symmetric history clamp into an asymmetric pair: LMR_HISTORY_DEEPEN_MAX (negative-history side, widened) vs LMR_HISTORY_EASE_MAX (positive side, unchanged so the trusted prefix keeps its depth). Lower LMR_HISTORY_DIVISOR to steepen the slope, and make the non-improving penalty a named constant that can be widened past 1 ply.
2. Build --features selstats binaries for a small candidate ladder and profile each with tools/diag/selectivity_profile.py (fixed depth 14 + fixed 2M nodes, bench-positions.epd, 64MB hash). Select the moderate point whose LMR reduction-ply distribution shifts right (more mass >=4 ply) without the re-search rate exploding; record the movement. Profile is selection only; SPRT is the arbiter.
3. Set the constants to the selected candidate; run cargo fmt --check, clippy -D warnings, cargo test --workspace.
4. Run self-play SPRT at tc=10+0.1 (elo0=-5, elo1=0) baseline (branch base) vs candidate via tools/strength/strength_test.py.
5. Record profile movement + SPRT verdict in BENCHMARKS.md. Retain on non-negative SPRT; on a negative, revert the constants to baseline and record the informative negative (AC#3). No cross-engine diffing (AC#4).
<!-- SECTION:PLAN:END -->
