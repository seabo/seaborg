---
id: TASK-97.1
title: 'A2: First-move-cutoff decomposition (reuse selstats)'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-29 18:45'
updated_date: '2026-07-29 18:59'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - tools/diag/selectivity_profile.py
  - engine/src/ordering.rs
parent_task_id: TASK-97
priority: high
type: spike
ordinal: 166000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track A / item A2. Hypothesis: most of the ordering loss is concentrated at no-TT-move nodes. Reuse tools/diag/selectivity_profile.py at fixed depth and fixed nodes — it already emits first-move-cutoff split by TT hit/miss, the cutoff move-index histogram, and TT availability, so no new code is needed. Decision value: confirms WHERE to aim (quantifies the 92.7% with-TT vs 86.4% no-TT cutoff gap against the ~67% of cutoffs that occur at no-TT nodes) and scopes A1. Run this first — it is the cheapest item and scopes the rest of Track A.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The selectivity profile is re-run at fixed depth and fixed nodes on bench-positions.epd, reporting first-move-cutoff split by TT-hit vs TT-miss, the cutoff move-index histogram, and TT-move-availability by node type
- [ ] #2 The share of total beta cutoffs occurring at no-TT-move nodes and the TT vs no-TT first-move-cutoff gap are quantified against the recorded baseline
- [ ] #3 No permanent engine change ships; only diagnostic output is produced
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Build the behaviour-transparent instrumented binary: RUSTFLAGS="-C target-cpu=native" cargo build --release --features selstats. A selstats build searches the identical tree as default, so the profile is unbiased.
2. Run tools/diag/selectivity_profile.py at both operating points on the fixed suite: --seaborg target/release/seaborg --suite tools/diag/bench-positions.epd --depth 14 --nodes 2000000 --hash 64 --out (gitignored *.json; no permanent artifact ships, satisfying no-engine-change).
3. AC#1 — read the three signals from each aggregate: first_move_cutoff_rate_tt vs _nott, cutoff_index_dist (move-index histogram), and tt_move_avail / _pv / _nonpv.
4. AC#2 — quantify the share of total beta cutoffs at no-TT nodes (cutoff_share_nott) and the TT vs no-TT first-move-cutoff gap; compare both to the recorded baseline (fixed-depth-14: 88.5% overall, 92.7% TT vs 86.4% no-TT, 67% of cutoffs at no-TT nodes; fixed-2M: 89.4%, avail 35.4%). Note any drift.
5. Record the decomposition and the A2 conclusion (is ordering loss concentrated at no-TT-move nodes? -> scopes A1 oracle ceiling) in implementation notes; refresh BENCHMARKS.md selectivity figures only if stable. No source changes; run fmt/clippy/test as the clean gate.
6. AC#3 — verify no permanent engine change ships; only diagnostic output.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Findings (A2 first-move-cutoff decomposition; reused tools/diag/selectivity_profile.py, no engine change).

Ran a --features selstats build over bench-positions.epd at fixed depth 14 and fixed 2M nodes, 64 MB hash. Every headline figure reproduces the TASK-97 recorded baseline exactly (fixed-depth: EBF 2.710, first-move-cutoff 88.5%, TT-avail 27.4% / non-PV 27.1%, TT-hit 92.7% vs miss 86.4%, misses 67% of cutoffs; fixed-2M: EBF 2.472, depth 16.25, 89.4%, avail 35.4% / non-PV 31.3%). Confirms the profile is stable and behaviour-transparent.

AC#1 signals captured: first-move-cutoff split by TT hit/miss, cutoff move-index histogram ([88.5 6.9 2.5 0.6 0.4 0.2 0.1 0.8] at depth 14), and TT-move availability by node type (pooled + PV/non-PV).

AC#2 decomposition (raw pooled counts): the TT vs no-TT first-move-cutoff gap is 6.3 pp (depth 14) / 6.2 pp (2M). Beyond the 67%/60% no-TT share of cutoffs, no-TT nodes carry 79.2% (depth 14) / 73.9% (2M) of all ORDERING WASTE (late cutoffs = fail-highs whose first move did not cut: 347,404 total at depth 14, 275,020 at no-TT nodes). The two effects compound: no-TT nodes both miss move-one more often AND are where most cutoffs happen. The cutoff-index histogram shows the waste is shallow (almost all misses cut by move 2-3), so the lever is the RIGHT FIRST MOVE at no-TT nodes, not deeper list search.

Conclusion: free/ordering-recoverable EBF, if any, lives overwhelmingly at no-TT-move nodes. This scopes A1 (oracle-ordering ceiling) to that population and points Track D at seeding ordering there (IIR / move-synthesis), not at raising TT-move availability (already ruled low-leverage).

AC#3: no engine change. Only diagnostic output produced; the *.json profile output is gitignored. A short Phase-3 write-up was added to the BENCHMARKS.md selectivity re-baseline section (documentation, not engine behaviour), consistent with the diag tool's stated workflow.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-29 18:59
---
Implementation handoff
Branch: task-97.1-first-move-cutoff-decomp
Worktree: /Users/seabo/seaborg-worktrees/task-97.1-first-move-cutoff-decomp
Base: 7af6f1edafab251c014ee6de82ed88f125afc85b
Implementation target: c1da3a7
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (no warnings)
- cargo test --workspace: PASS (all suites green; no source changed)
- Diagnostic reproduced: selectivity_profile.py at depth 14 + 2M nodes matches recorded baseline exactly
Known failures: none

Scope note for reviewer: spike / diagnostic only. No engine source changed (git diff vs base touches BENCHMARKS.md only; the *.json profile artifact is gitignored). AC#3 satisfied by construction. ACs #1/#2 evidenced by the appended findings notes and the BENCHMARKS.md Phase-3 table.
---
<!-- COMMENTS:END -->
