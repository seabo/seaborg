---
id: TASK-89.2
title: 'Engage LMR earlier: lower the move threshold'
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-27 15:08'
updated_date: '2026-07-28 09:26'
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
ordinal: 153000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 2. Mechanism: apply the 'ordering is trustworthy' argument to the front of the move tail - start reducing before the current threshold and/or reduce the first few non-PV moves, claiming depth on moves ordering already ranks low. Cheap; pairs naturally with 89.1 but is gated separately to attribute the gain. Tunable: LMR_MOVE_THRESHOLD in engine/src/search.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 LMR_MOVE_THRESHOLD (and/or first-few-non-PV reduction) is swept; chosen value measured by self-play SPRT at 10s+0.1s vs the pre-change baseline and recorded in BENCHMARKS.md
- [x] #2 The selstats profile confirms depth reached / EBF improved without the re-search rate exploding
- [x] #3 Retained only on a non-negative SPRT; a negative is recorded and reverted
- [x] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Selstats sweep of LMR_MOVE_THRESHOLD candidate values (3 -> 2, 3 -> 1) with tools/diag/selectivity_profile.py at fixed depth 14 and fixed 2M nodes over bench-positions.epd. Confirm the intended signal: engaging LMR earlier raises the mean reduction / shifts the reduction distribution and lowers EBF (more depth per node) without the re-search rate exploding. Pick the candidate that best trades a modest re-search-rate rise for lower EBF.
2. Build the target-cpu=native release for the baseline (threshold=3) and the chosen candidate; record binary sha256 for each.
3. Self-play SPRT at tc=10+0.1 (64 MB hash, 1 worker/engine) with tools/strength/strength_test.py, elo0=-5 elo1=0 (no-regression gate), mirroring the TASK-89.1 methodology.
4. Record the sweep, profile movement, and SPRT verdict in BENCHMARKS.md as a new 'Engage LMR earlier' subsection.
5. Retain only on a measured non-negative SPRT; a negative is recorded and the constant reverted to 3. No cross-engine diffing; conclusion from Seaborg's own signals + self-play.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Outcome: informative NEGATIVE, reverted. No code change lands; LMR_MOVE_THRESHOLD stays 3.

Sweep (tools/diag/selectivity_profile.py, fixed depth 14 + fixed 2M nodes, bench-positions.epd, 64 MB hash):
- threshold 3 (base): fixed-depth EBF 2.710, re-search 1.75%; fixed-nodes depth 16.25, EBF 2.472.
- threshold 2 (cand): fixed-depth EBF 2.614, re-search 1.79%; fixed-nodes depth 17.70 (+1.45 ply), EBF 2.375.
- threshold 1: re-search 1.99%, fixed-nodes EBF 4.066 (noisy) -> rejected as riskier; picked 2 as the moderate step.

The profile signal (AC#2) moved as predicted for threshold=2: more depth per node, EBF down, re-search rate essentially flat. But strength did not follow.

SPRT (AC#1, AC#3, AC#4): tc=10+0.1, 64 MB hash, 1 worker/engine, concurrency 6, elo0=-5 elo1=0 alpha=beta=0.05.
- Baseline sha256 ae25648d (threshold=3); Candidate sha256 348d8768 (threshold=2).
- FAIL: LLR -2.95 crossed lower bound -2.94; Elo -10.43 +/- 6.36; 5964 games W-D-L 1479-2827-1658; pentanomial 217-793-1124-648-200; 0 crashes/forfeits.

Interpretation: the full-depth prefix of three moves is load-bearing. The re-search rate stays flat because ordering is usually right, but the rare positions where the 3rd move (first alternative past the top two) is decisive are exactly the game-deciding ones, and a reduced scout of them is not corrected often enough to pay for the depth. Counterpart to TASK-89.1: reduce the tail *harder* wins (+26), start reducing *earlier* does not.

Conclusion derived from Seaborg's own selstats profile + self-play only; no cross-engine diffing (AC#4). Full write-up appended to BENCHMARKS.md.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-28 09:20
---
Implementation handoff
Branch: task-89.2-lmr-earlier-threshold
Worktree: /Users/seabo/seaborg-worktrees/task-89.2-lmr-earlier-threshold
Base: 644153ae4d9413cc0243e1211f0ce7641719979a
Implementation target: 2468e0742f34bcb9bd45b215442f537ea0cd30f4
Resolved findings: none
Outcome: informative NEGATIVE; LMR_MOVE_THRESHOLD reverted to 3 (no code change vs base); only BENCHMARKS.md added.
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (no warnings)
- cargo test --workspace: PASS (476 + 157 + 57 + others, 0 failed)
- selstats sweep + self-play SPRT: candidate threshold=2 FAIL, -10.43 +/- 6.36 Elo, 5964 games (see BENCHMARKS.md 'Engage LMR earlier')
Known failures: none
---

author: @george
created: 2026-07-28 09:26
---
Review attempt: 1
Reviewed branch: task-89.2-lmr-earlier-threshold
Reviewed implementation: 2468e0742f34bcb9bd45b215442f537ea0cd30f4
Base: 644153ae4d9413cc0243e1211f0ce7641719979a
Verdict: approved

Outcome is an informative NEGATIVE handled exactly as AC#3 requires: measured, recorded in BENCHMARKS.md, and reverted. The base->target diff touches only BENCHMARKS.md and the task file — engine code is byte-identical (LMR_MOVE_THRESHOLD stays 3, engine/src/search.rs:322), so no hot-path benchmark is warranted.

Acceptance criteria:
- AC#1 PASS — threshold swept 3/2/1 via selectivity_profile.py; candidate 2 measured by self-play SPRT at tc=10+0.1 vs base 644153a and recorded in BENCHMARKS.md.
- AC#2 PASS — selstats profile shows depth/EBF improved (fixed-nodes 16.25->17.70 ply, EBF 2.472->2.375) with re-search rate flat (1.75%->1.79%); recorded.
- AC#3 PASS — SPRT negative (-10.43 +/- 6.36 Elo, FAIL); recorded and constant reverted to 3 (zero engine-code diff).
- AC#4 PASS — conclusion from Seaborg's own selstats + self-play only; no cross-engine diffing.

Internal consistency checked: WDL 1479-2827-1658 = 5964 games; pentanomial 217-793-1124-648-200 = 2982 pairs = 5964. Target is an ancestor of the branch tip; the only later commit is handoff metadata.

Verification on target 2468e07:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (no warnings)
- cargo test --workspace: PASS (476 + 157 + 57 + suites; 0 failed)
- git diff base..target: BENCHMARKS.md + task file only; no engine code change
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Informative negative, reverted — no engine code lands. Swept LMR_MOVE_THRESHOLD (3->2->1) with tools/diag/selectivity_profile.py; candidate 2 showed the predicted profile movement (fixed-nodes +1.45 ply, EBF 2.472->2.375, re-search rate flat 1.75%->1.79%) but a self-play SPRT at tc=10+0.1 (elo0=-5,elo1=0) FAILED: -10.43 +/- 6.36 Elo over 5964 games (WDL 1479-2827-1658; LLR -2.95 crossed -2.94). Threshold reverted to 3 (base=target for engine code); only BENCHMARKS.md 'Engage LMR earlier' write-up added. The full-depth prefix of three moves is load-bearing; re-search rate is the wrong safety signal. Verified fmt/clippy/test all green on target 2468e07.
<!-- SECTION:FINAL_SUMMARY:END -->
