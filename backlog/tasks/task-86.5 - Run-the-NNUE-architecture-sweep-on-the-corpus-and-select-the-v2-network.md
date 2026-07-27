---
id: TASK-86.5
title: Run the NNUE architecture sweep on the corpus and select the v2 network
status: To Do
assignee: []
created_date: '2026-07-25 12:24'
updated_date: '2026-07-27 17:47'
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
