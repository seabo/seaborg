---
id: TASK-64.16.8
title: Evaluate cross-worker root-result voting
status: To Do
assignee: []
created_date: '2026-07-19 23:25'
labels:
  - search
  - concurrency
  - strength
  - experiment
dependencies:
  - TASK-64.16.7
references:
  - engine/src/search.rs
  - engine/src/info.rs
  - tools/strength/strength_test.py
parent_task_id: TASK-64.16
priority: low
type: spike
ordinal: 99000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Determine whether selecting an official root result using completed helper results improves strength over the conservative master-authoritative Lazy SMP baseline. Helper results are not automatically comparable: they may come from different depths, aspiration windows, completion times, and diversified schedules. A shallow or bounded helper result must never casually override a deeper exact master result.

This is an evidence-gated experiment. The default remains the master last completed iteration. Implement a documented eligibility and voting model, validate score and PV coherence, and retain it only if it produces a statistically meaningful strength gain without protocol or lifecycle regressions. If no policy earns retention, close the task with the negative result and keep master authority.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Only fully completed root iterations with an exact score, legal best move, and coherent PV are eligible to influence official result selection
- [ ] #2 The comparison policy explicitly handles depth, mate distance, score, agreement count, worker role, and ties; interrupted or partially widened aspiration searches cannot vote
- [ ] #3 A shallower helper cannot override a deeper authoritative result except under a separately justified and tested rule
- [ ] #4 The selected score, depth, best move, and PV always come from one eligible result rather than being assembled from different workers
- [ ] #5 Deterministic tests cover disagreement, ties, mate scores, different depths, cancellation, missing PVs, illegal helper moves, and no eligible helper
- [ ] #6 Master-only selection and each candidate voting policy are compared with statistically meaningful paired strength tests at more than one worker count
- [ ] #7 Voting is retained only on a demonstrated strength win with no correctness or time-loss regression; otherwise the implementation is removed and the negative conclusion is recorded
<!-- AC:END -->
