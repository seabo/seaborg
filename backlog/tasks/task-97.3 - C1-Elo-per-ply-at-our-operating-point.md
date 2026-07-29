---
id: TASK-97.3
title: 'C1: Elo-per-ply at our operating point'
status: To Do
assignee: []
created_date: '2026-07-29 18:46'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - tools/strength_test.py
  - BENCHMARKS.md
parent_task_id: TASK-97
priority: high
type: spike
ordinal: 168000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track C / item C1. Hypothesis: an extra ply is worth ~40–60 Elo here; if so, the 5–6 ply effective-depth gap is the +200–300 and ordering/machinery is the way to reach it. Method: strength_test.py, same net, --limit nodes=N vs nodes=1.5N (and depth=D vs D+1), SPRT. Self-play only — no cross-engine. Metric: Elo per +50% nodes / per +1 ply. Decision: sets the value of every selectivity/ordering win and sanity-checks the whole thesis. If a ply is worth little, the depth story is wrong and we rethink (feeds guardrail 4).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Self-play SPRT measures the Elo delta of nodes=N vs nodes=1.5N and of depth=D vs depth=D+1 with the same network, at recorded limits
- [ ] #2 Elo-per-ply (and Elo per +50% nodes) at the current operating point is reported with confidence bounds
- [ ] #3 The result explicitly feeds guardrail 4: whether a ply is worth enough to justify pursuing depth via ordering/machinery
<!-- AC:END -->
