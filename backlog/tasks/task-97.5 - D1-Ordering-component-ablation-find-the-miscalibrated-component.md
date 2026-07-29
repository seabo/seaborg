---
id: TASK-97.5
title: 'D1: Ordering-component ablation (find the miscalibrated component)'
status: To Do
assignee: []
created_date: '2026-07-29 18:46'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - engine/src/ordering.rs
  - tools/diag/selectivity_profile.py
parent_task_id: TASK-97
priority: medium
type: spike
ordinal: 170000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track D / item D1. Hypothesis: some quiet-ordering components are miscalibrated or net-neutral (cheap wins hiding as losses). Method: ablate each — main history, continuation history at distances 1 and 2, capture history, counter-move, killers — via the existing ordering-ablation constants; measure first-move-cutoff + node count at fixed depth, then SPRT the promising deltas. Decision: any component whose removal RAISES first-move-cutoff is miscalibrated → recalibrate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each ordering component (main history, continuation history dist 1 and 2, capture history, counter-move, killers) is independently ablated and first-move-cutoff plus node count at fixed depth are reported per ablation
- [ ] #2 Any component whose removal does not lower (or actually raises) first-move-cutoff is flagged as miscalibrated
- [ ] #3 Flagged recalibration candidates are taken to a self-play SPRT at tc=10+0.1 before any change ships
<!-- AC:END -->
