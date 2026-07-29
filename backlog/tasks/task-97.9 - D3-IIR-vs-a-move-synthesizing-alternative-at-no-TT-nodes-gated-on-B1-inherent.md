---
id: TASK-97.9
title: >-
  D3: IIR vs a move-synthesizing alternative at no-TT nodes (gated on B1
  inherent)
status: To Do
assignee: []
created_date: '2026-07-29 18:46'
labels:
  - search
  - selectivity
  - investigation
dependencies:
  - TASK-97.2
references:
  - engine/src/search.rs
parent_task_id: TASK-97
priority: low
type: spike
ordinal: 174000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track D / item D3. Runs ONLY if B1 (TASK-97.2) shows the blind nodes are inherent (not evictable). At the ~70% blind nodes we currently just REDUCE (IIR, the measured +28 from TASK-52). Hypothesis: a better first move there (not just less depth) may order better. Method: compare our IIR against an internal-iterative-deepening variant at no-TT nodes; selectivity-profile first-move-cutoff + SPRT. Note the prior — strong engines moved TO IIR FROM IID — so treat this as "test, do not assume." It is here because the blind-node population is large enough to be worth one falsification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Gated on B1 showing no-TT nodes are inherent (not eviction-fixable); if B1 shows eviction-limited this task is closed as not-applicable with that finding recorded
- [ ] #2 At no-TT-move nodes, an internal-iterative-deepening (move-synthesizing) variant is compared against current IIR on first-move-cutoff at fixed depth and via a self-play SPRT at tc=10+0.1
- [ ] #3 The prior (strong engines moved from IID to IIR) is treated as a hypothesis to falsify, not an assumption; the SPRT result decides and is recorded
<!-- AC:END -->
