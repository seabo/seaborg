---
id: TASK-89.1
title: 'Reduce harder in LMR: raise the reduction base and growth'
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-27 15:08'
updated_date: '2026-07-27 21:02'
labels:
  - search
  - selectivity
  - strength
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-89
priority: high
type: feature
ordinal: 152000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-88 experiment 1 (top-ranked). Mechanism: raise the reduction so the mean climbs above ~2.1 ply and the LMR re-search rate rises from ~1.7 percent toward a healthier band; fewer nodes per subtree buys more iterative-deepening depth at equal time. The same ordering that yields an 88.8 percent first-move-cutoff rate is trustworthy enough to reduce the tail harder. Tunables: LMR_BASE, LMR_DIVISOR in engine/src/search.rs. Risk: if strength drops, the reductions were already near-optimal - an informative result in itself.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 LMR_BASE and/or LMR_DIVISOR are swept; the chosen values are measured by self-play SPRT at 10s+0.1s against the pre-change baseline and recorded in BENCHMARKS.md
- [x] #2 The selstats profile confirms the re-search rate rose toward a healthier band and mean reduction increased, without the re-search rate exploding
- [x] #3 Retained only on a non-negative SPRT; a strength drop is recorded as an informative negative (reductions already near-optimal) and reverted
- [x] #4 Conclusion derived from Seaborg's own signals and self-play; no cross-engine behaviour diffing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Build a baseline selstats binary at the branch base and measure the pre-change LMR re-search rate and mean reduction with tools/diag/selectivity_profile.py (fixed depth 14 + 2M nodes, bench-positions.epd, 64MB hash), reproducing the TASK-88 reading (~1.7%, ~2.1 ply).
2. Sweep LMR_BASE / LMR_DIVISOR over a small grid (raise base and/or steepen growth by lowering the divisor). For each candidate, rebuild a selstats binary and measure re-search rate + mean reduction. Select the candidate that lifts the mean reduction above ~2.1 ply and the re-search rate toward a healthier band without exploding it.
3. Apply the chosen constants to engine/src/search.rs (default build path).
4. Build optimized release binaries at the base (baseline) and with the chosen constants (candidate); run an authoritative self-play SPRT at tc=10+0.1 via tools/strength/strength_test.py, concurrency scaled to the P-cores.
5. Re-run the selstats profile on the retained candidate to confirm AC#2: re-search rate rose and mean reduction increased without the re-search rate exploding.
6. Retain only on a non-negative SPRT; on a strength drop, revert the constants and record the informative negative (reductions already near-optimal). Record the sweep, chosen values, SPRT verdict, and profile movement in BENCHMARKS.md.
7. Run the repo-required checks (fmt/clippy --all-features/test) and hand off to review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Raised the LMR reduction curve: LMR_BASE 0.5->1.0, LMR_DIVISOR 2.0->1.5 in engine/src/search.rs (ranked experiment 1 from the TASK-88 selectivity profile).

Sweep (self-instrumented, no games): profiled 10 (base,divisor) points via tools/diag/selectivity_profile.py at fixed depth 14. Key finding — the LMR re-search rate is nearly insensitive to the reduction amount (1.6% baseline, only ~1.9% even at an extreme base=2.0/div=1.0): ~98% of reduced scouts fail low as ordering predicts across the whole range, confirming the pre-change schedule left depth unclaimed rather than sitting at the edge of safety. Chose base=1.0/div=1.5 as a moderate point raising both terms.

Profile movement (baseline->candidate): mean reduction 2.16->2.45 ply (fixed depth), 2.07->2.44 (fixed 2M nodes); re-search rate 1.60->1.75% / 1.99->2.73% (rose, did not explode); reduction tail >=4 ply 6.6->15.8% / 5.6->16.2%; EBF at equal nodes 2.86->2.47 (the depth-buying mechanism). Satisfies AC#2.

SPRT (AC#1/#3): authoritative self-play, tc=10+0.1, 64MB hash, 1 worker/engine, concurrency 6, fastchess 1.5.0, openings-v1.epd, target-cpu=native release. Baseline git:153a720 (base constants) vs candidate git:6b5100d. Verdict PASS, LLR 2.94 crossed +2.94 bound at 1856 games; Elo +26.07 +/- 11.47 (pentanomial), W-D-L 569-857-430, pentanomial 44-201-346-246-91, 0 crashes/forfeits. Retained. Recorded in BENCHMARKS.md ('Selectivity tuning experiments'). AC#4 satisfied: conclusion rests solely on Seaborg self-play and self-instrumentation; no cross-engine diffing.

Test maintenance: child_mate_windows_preserve_distance_parity re-pinned from depth 7 to depth 8 — the harder schedule delays when this position's forced mate first surfaces (depth 7 now returns a non-mate Cp score, depth 8 surfaces Mate(7)). The test's subject (mate-distance parity plumbing through an aspiration fail-high re-search) and the true mate distance (mate in 4) are unchanged; only the fixture depth moved. Verified via a temporary depth-14 probe.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-27 20:31
---
Implementation handoff
Branch: task-89.1-lmr-reduce-harder
Worktree: /Users/seabo/seaborg-worktrees/task-89.1-lmr-reduce-harder
Base: 153a7206c3f3cb652d7fda97498a95bba96f4dcc
Implementation target: ea6a3a1
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (450 engine + 157 lichess + workspace suites; 0 failed, 2 ignored)
- Authoritative SPRT tc=10+0.1: PASS, +26.07 +/- 11.47 Elo, 1856 games (569-857-430), LLR 2.94. Report + games.pgn under /tmp/lmr89/sprt (git-ignored; key figures in BENCHMARKS.md). SPRT binaries built from base 153a720 and candidate 6b5100d; the later test-only commit 169b9a9 does not alter the release binary.
Known failures: none
---

author: @claude
created: 2026-07-27 21:02
---
Review attempt: 1
Reviewed branch: task-89.1-lmr-reduce-harder
Reviewed implementation: ea6a3a1 (code target; approval commit is task-only, branch tip)
Verdict: approved -> Ready to Merge

Immutability: base 153a7206 is an ancestor of target ea6a3a1, which is an ancestor of tip b7263fe; ea6a3a1..b7263fe changes only the task file. No implementation file changes after the target.

Scope: engine/src/search.rs (LMR_BASE 0.5->1.0, LMR_DIVISOR 2.0->1.5 + self-contained comments), engine/src/search/tests.rs (mate-parity fixture depth 7->8), BENCHMARKS.md, task file. No unrelated changes. No #[allow] added.

Required checks on target code:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (fresh CARGO_TARGET_DIR=/tmp/lmr89-clippy, clean)
- cargo test --workspace: pass (450 engine + 157 lichess + 57 + others; 0 failed, 2 ignored); child_mate_windows_preserve_distance_parity passes at depth 8 with Score::mate(7)/'score mate 4'

AC#2 independently reproduced: built base (153a720) and candidate selstats binaries, ran tools/diag/selectivity_profile.py --suite bench-positions.epd --depth 14 --nodes 2000000 --hash 64.
- Base:      depth14 re-search 1.60%, mean red 2.16 ply, EBF 2.81; 2M nodes 1.99%, 2.07 ply, EBF 2.86
- Candidate: depth14 re-search 1.75%, mean red 2.45 ply, EBF 2.71; 2M nodes 2.73%, 2.44 ply, EBF 2.47
- Reduction tail >=4ply: 15.8% (depth14) / 16.2% (2M nodes)
Every BENCHMARKS.md figure reproduced exactly. Re-search rose without exploding; mean reduction increased. AC#2 met.

AC#1/#3: recorded self-play SPRT (tc=10+0.1, 64MB, elo0=-5/elo1=0 no-regression gate, fastchess 1.5.0, openings-v1.epd) PASS, +26.07+/-11.47 Elo, 1856 games (569-857-430), LLR 2.94. A full re-run is impractical in review (hours); the reviewed code equals the tested candidate 6b5100d (169b9a9 test-only + ea6a3a1 docs-only do not alter the release binary), the gate is a valid non-regression test the change accepted, and the EBF-at-equal-nodes mechanism it bet on reproduces (2.86->2.47). Retained on a non-negative SPRT per AC#3.

AC#4: conclusion rests solely on Seaborg self-play + self-instrumentation; no cross-engine diffing.

Hot path: LMR reduction constants affect search selectivity, not movegen/perft; perft/movegen benchmarks not applicable.

All four acceptance criteria proven. Approved.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Raised the LMR reduction curve (LMR_BASE 0.5->1.0, LMR_DIVISOR 2.0->1.5 in engine/src/search.rs), re-pinned the child_mate_windows_preserve_distance_parity fixture from depth 7 to 8 (the harder schedule surfaces this position's mate one iteration later; true mate distance and the parity subject unchanged), and recorded the sweep/profile/SPRT in BENCHMARKS.md. Verified: fmt clean; clippy --all-features clean on a fresh CARGO_TARGET_DIR; cargo test --workspace all pass (450 engine + 157 lichess + others, 0 failed). AC#2 independently reproduced by rebuilding base (153a720) and candidate selstats binaries and running tools/diag/selectivity_profile.py at fixed depth 14 + 2M nodes: mean reduction 2.16->2.45 / 2.07->2.44 ply, re-search rate 1.60->1.75% / 1.99->2.73% (rose, did not explode), reduction tail >=4ply 6.6->15.8% / 5.6->16.2%, EBF at equal nodes 2.86->2.47 -- every BENCHMARKS.md figure reproduced exactly. AC#1/#3 rest on the recorded self-play SPRT (tc=10+0.1, elo0=-5/elo1=0 no-regression gate): PASS +26.07+/-11.47 Elo over 1856 games; the reviewed code equals the tested candidate 6b5100d (later commits are test/docs-only, not affecting the release binary), and the EBF mechanism the SPRT bet on reproduces. AC#4 self-play + self-instrumentation only.
<!-- SECTION:FINAL_SUMMARY:END -->
