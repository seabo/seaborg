---
id: TASK-86.5
title: Run the NNUE architecture sweep on the corpus and select the v2 network
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-25 12:24'
updated_date: '2026-07-27 22:47'
labels:
  - nnue
dependencies:
  - TASK-86.3
  - TASK-86.4
  - TASK-81
  - TASK-86.7
parent_task_id: TASK-86
priority: high
ordinal: 147000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Execute the architecture sweep defined by the methodology decision doc (TASK-86.3) against the fixed corpus from TASK-81, and select the network to promote. Sweep the axes that matter, roughly one factor at a time so results are attributable: feature-transformer width H (e.g. 256 -> 512 -> 1024 -> 2048), activation (CReLU vs SCReLU), output-stack depth and output buckets (none vs 8), and dense-tail quantization. For each candidate record post-QAT quantized validation loss and realized in-engine single-thread NPS, plot the loss/NPS Pareto frontier, and run fixed-time-control SPRT on the frontier finalists against the current gen-002 default (and against each other). Output a selected network plus a report that also reads off whether the corpus is label-limited or capacity-limited at the chosen size.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A loss-vs-NPS Pareto frontier is produced over the swept architectures following the TASK-86.3 protocol, with the swept factors and any coverage limits recorded
- [ ] #2 Frontier finalists are evaluated by fixed-time-control SPRT against the gen-002 default, with results and attribution recorded in BENCHMARKS.md
- [ ] #3 A single network is selected with a written rationale grounded in fixed-TC Elo, and the report states whether the corpus is label-limited or capacity-limited at that size
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Phase 1 (this session, bounded): run the sweep.py screen on the rig.
1. Sync rig seaborg clone to this task branch's tip; build release with target-cpu=native.
2. Locate corpus-gen-002 + provenance manifest + baseline gen-002 net; assemble a fixed NPS position suite.
3. Run tools/trainer/sweep.py --device cuda: enumerate one-factor-at-a-time candidates, train+export each on the leak-free by-shard split, record post-QAT val loss + single-thread NPS with attribution, compute loss/NPS Pareto frontier, select finalists, emit strength_test.py commands.
4. Install fastchess on the rig (prereq for phase 2), verify.
5. Commit the frontier report + finalist list under the task branch; record coverage limits (AC#1). PAUSE for go/no-go before the multi-day SPRT.
Phase 2 (after approval): run finalist SPRT vs gen-002 (+head-to-heads); record results/attribution in BENCHMARKS.md (AC#2).
Phase 3: select the net by fixed-TC Elo, write rationale + label-limited vs capacity-limited read, promote (AC#3).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Paused pending TASK-86.8 (parallelize the packed dataloader). Phase-0 rig setup done and preserved on this branch: rig clone synced to this branch, target-cpu=native release built, by-shard split pre-flighted (92.8M train / 10.3M val, shards 000+018 held out, deterministic), sweep width axis extended to 1024 (14 candidates). Measured per-epoch ~2.5-3.2 min, GPU ~19% (single-thread dataloader is the bottleneck), so the 14-candidate screen is an overnight job at the default budget; parallelizing the loader first (86.8) makes it ~3-4h. Resume here after 86.8 lands: rebuild engine, run tools/trainer/sweep.py on corpus-gen-002.
<!-- SECTION:NOTES:END -->
