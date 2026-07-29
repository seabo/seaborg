---
id: TASK-97.1
title: 'A2: First-move-cutoff decomposition (reuse selstats)'
status: To Do
assignee: []
created_date: '2026-07-29 18:45'
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
