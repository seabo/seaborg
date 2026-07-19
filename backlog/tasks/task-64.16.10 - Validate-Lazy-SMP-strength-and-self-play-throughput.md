---
id: TASK-64.16.10
title: Validate Lazy SMP strength and self-play throughput
status: To Do
assignee: []
created_date: '2026-07-19 23:25'
labels:
  - search
  - concurrency
  - strength
  - datagen
dependencies:
  - TASK-64.16.8
  - TASK-64.16.9
references:
  - tools/strength/strength_test.py
  - docs/strength-testing.md
  - engine/src/search.rs
  - engine/src/options.rs
parent_task_id: TASK-64.16
priority: high
type: task
ordinal: 101000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Run the closing evaluation for the Lazy SMP programme and record which configurations should be used for strongest play and for maximum self-play data generation. More threads per game can increase game strength while reducing the number of independent games produced per machine; these are different objectives and must not be collapsed into one throughput number.

Use repository-owned paired testing with identical binaries except for the intended thread configuration or policy. Control total hardware occupancy so an SMP build is not compared against a baseline receiving different aggregate CPU or hash resources by accident. Report uncertainty rather than treating a small finite match as proof.

This task validates and documents the final retained system. It does not introduce unmeasured new search policy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Paired strength tests compare one, two, four, and eight workers at representative fast and longer time controls with equal per-game hash and documented CPU allocation
- [ ] #2 Results record games, W-D-L, Elo estimate and uncertainty or SPRT bounds, time forfeits, illegal moves, crashes, hangs, and average completed depth
- [ ] #3 Scaling results record NPS and useful depth gain separately from Elo so redundant work is visible
- [ ] #4 Self-play data generation compares positions or games per wall-clock hour for one multi-threaded game and multiple concurrent one-thread games under equal total cores and memory
- [ ] #5 The recommended strongest-play and maximum-datagen configurations are recorded separately, including Threads and Hash guidance
- [ ] #6 A soak run of the recommended configuration completes with zero correctness or lifecycle failures
- [ ] #7 The final README and engine option documentation claim only the capabilities and scaling actually demonstrated
- [ ] #8 TASK-64.16 receives a closing summary covering architecture, retained and rejected policies, one-thread regression status, scaling, strength, and datagen throughput
<!-- AC:END -->
