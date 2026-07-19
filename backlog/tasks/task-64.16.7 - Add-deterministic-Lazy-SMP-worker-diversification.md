---
id: TASK-64.16.7
title: Add deterministic Lazy SMP worker diversification
status: To Do
assignee: []
created_date: '2026-07-19 23:25'
labels:
  - search
  - concurrency
  - strength
  - tuning
dependencies:
  - TASK-40
  - TASK-51
  - TASK-52
  - TASK-64.5
  - TASK-64.16.6
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
  - engine/src/tt.rs
  - tools/strength/strength_test.py
  - docs/strength-testing.md
parent_task_id: TASK-64.16
priority: medium
type: feature
ordinal: 98000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make helper workers explore usefully different parts of the tree after the homogeneous system is correct and the major aspiration, reduction, and time-management shape has landed. Identical workers can diverge through TT races, but deliberate thread-indexed variation can reduce redundant work and improve the information helpers contribute.

Evaluate policies independently rather than landing an opaque bundle. Candidates include deterministic root-order rotations, helper iteration-depth schedules, stable worker-specific ordering perturbations, aspiration-window variation, or carefully bounded reduction variation. Any restricted or excluded move search must not publish a score as though it described the full position. Uncontrolled operating-system randomness is not an acceptable test dependency.

This is a strength task. Each retained policy must be attributable, reproducible when requested, neutral at Threads=1, and measured against the immediately preceding accepted policy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Worker diversification is a deterministic function of documented search inputs such as worker index, search identity, iteration, or root ordering
- [ ] #2 Threads=1 takes the unchanged master path and incurs no diversification-dependent result or measurable hot-path cost
- [ ] #3 Each candidate policy is evaluated separately for node overlap, depth reached, TT hit and replacement behavior, throughput, and Elo before being retained
- [ ] #4 No restricted, excluded, or otherwise non-equivalent helper search publishes an Exact or bound entry under the unrestricted position key
- [ ] #5 Diversification preserves legal moves, completed-iteration result semantics, mate-score invariants, repetition and fifty-move safeguards, and prompt team cancellation
- [ ] #6 Fixed-seed or deterministic tests reproduce each retained worker schedule and show that at least two helpers do meaningfully different work
- [ ] #7 The final retained policy has statistically meaningful non-negative strength evidence at representative worker counts and time controls, recorded in the task notes
<!-- AC:END -->
