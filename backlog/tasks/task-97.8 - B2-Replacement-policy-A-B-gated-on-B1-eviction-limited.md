---
id: TASK-97.8
title: 'B2: Replacement-policy A/B (gated on B1 eviction-limited)'
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
  - engine/src/tt.rs
parent_task_id: TASK-97
priority: medium
type: enhancement
ordinal: 173000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track B / item B2. Runs ONLY if B1 (TASK-97.2) shows the blind-node population is eviction-limited. Hypothesis: our current replacement leaves depth-preferred entries behind; a proper aging/bucket scheme raises TT-move-availability at fixed size. Method: A/B two replacement policies at fixed Hash, selectivity profile + SPRT at tc=10+0.1. Decision: SPRT-positive → bank it; it is soundness-free ordering Elo.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Gated on B1 showing eviction-limited; if B1 shows inherent first-visits this task is closed as not-applicable with that finding recorded
- [ ] #2 At fixed Hash, an alternative replacement/aging policy is A/B tested against current: TT-move-availability and first-move-cutoff at fixed nodes, plus a self-play SPRT at tc=10+0.1
- [ ] #3 A positive SPRT is required before the new policy ships; a non-positive result reverts and records the finding
<!-- AC:END -->
