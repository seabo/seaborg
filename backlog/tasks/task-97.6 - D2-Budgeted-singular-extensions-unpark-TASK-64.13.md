---
id: TASK-97.6
title: 'D2: Budgeted singular extensions (unpark TASK-64.13)'
status: To Do
assignee: []
created_date: '2026-07-29 18:46'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-97
priority: medium
type: feature
ordinal: 171000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track D / item D2. The parked machinery from TASK-64.13 (singular extensions runaway + measured ~neutral, parked at Needs Human). Hypothesis: singular extensions are a real +tens lever once a path-extension budget stops the runaway that parked them. Method: instrument how often the TT move is "singular" (excluded-search fails low by the margin) across our tree, and the depth a budget-capped extension adds. If singularity is common and the budget bounds explosion, SPRT it. Decision: common + bounded → build it properly; it is depth-on-the-critical-line, not symptom-forcing. See TASK-64.13 for the prior runaway/neutral result.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The frequency that the TT move is singular (excluded-search fails low by the margin) across the tree is instrumented and reported, along with the extra depth a path-extension-budgeted extension adds
- [ ] #2 Whether a path-extension budget bounds the singular-chain explosion that parked TASK-64.13 is demonstrated
- [ ] #3 If singularity is common and the budget bounds explosion, a budgeted implementation clears a self-play SPRT at tc=10+0.1 before shipping; otherwise the negative finding is recorded against TASK-64.13
<!-- AC:END -->
