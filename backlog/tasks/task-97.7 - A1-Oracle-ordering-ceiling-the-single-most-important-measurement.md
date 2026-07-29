---
id: TASK-97.7
title: 'A1: Oracle-ordering ceiling (the single most important measurement)'
status: To Do
assignee: []
created_date: '2026-07-29 18:46'
labels:
  - search
  - selectivity
  - investigation
dependencies:
  - TASK-97.1
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
  - engine/src/pv_table.rs
parent_task_id: TASK-97
priority: high
type: spike
ordinal: 172000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track A / item A1 — the single most important measurement in the investigation. Hypothesis: a large fraction of our EBF gap is pure ordering waste, recoverable with zero soundness cost; if we seed every node with its TRUE best move first, EBF collapses toward the minimal-tree floor (sqrt b). Method: a two-pass instrumented build. Pass 1 does a full search and records the best move per position/node (or harvests from a deep TT). Pass 2 re-searches the same fixed depth with those moves forced to the front of ordering and counts nodes. Compare EBF real → oracle at fixed depth 12–16 on bench-positions.epd. Metrics: EBF_real, EBF_oracle, EBF_frontier (~2.01). Decision: EBF_real − EBF_oracle is the ceiling on what ordering ALONE can buy (free — do it first); EBF_oracle − 2.01 is the part that is genuinely pruning/eval (flywheel-gated — defer). This draws the line the whole strategy hinges on. Needs a new build; highest value. Depends on A2 (TASK-97.1) which scopes it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A two-pass oracle-ordering build measures node counts at fixed depth (12–16) on bench-positions.epd with true best moves forced to the front of ordering at every node
- [ ] #2 EBF_real, EBF_oracle, and the frontier reference (~2.01) are reported, and the split EBF_real − EBF_oracle (free ordering headroom) versus EBF_oracle − frontier (eval/pruning-limited) is stated
- [ ] #3 The instrumentation is temporary; no permanent engine change ships
- [ ] #4 The result explicitly feeds guardrail 4: whether the free-EBF ordering lever is large enough to pursue
<!-- AC:END -->
