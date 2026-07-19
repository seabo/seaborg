---
id: TASK-64.16.6
title: Stress and harden multi-worker search lifecycle correctness
status: To Do
assignee: []
created_date: '2026-07-19 23:24'
labels:
  - search
  - concurrency
  - testing
  - robustness
dependencies:
  - TASK-64.16.5
references:
  - engine/src/search.rs
  - engine/src/engine.rs
  - engine/src/game.rs
  - engine/src/tt.rs
  - tools/strength/strength_test.py
  - docs/strength-testing.md
parent_task_id: TASK-64.16
priority: high
type: task
ordinal: 97000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Exercise the homogeneous Lazy SMP system under adverse schedules and long-running protocol workloads before adding strength-oriented worker differences. Unit tests that merely start several identical workers are insufficient to establish that stop, completion, resource replacement, and panic paths remain correct across rare interleavings.

Build deterministic fault-injection seams where practical, plus bounded randomized stress and real FastChess workloads. The test programme must distinguish a genuine search hang from a driver or input lifecycle failure and must leave actionable artifacts when a subprocess fails. Heavy stress that is unsuitable for every CI run should have a documented reproducible command and a smaller CI smoke counterpart.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deterministic tests cover delayed master and helper exits, cancellation during each lifecycle phase, worker panic, partial spawn failure, completion racing stop, and repeated drop or wait paths
- [ ] #2 Repeated go, stop, replacement go, setoption Threads, setoption Hash, ucinewgame, EOF, and quit sequences never detach a worker or race hash clearing and resizing
- [ ] #3 Stress covers one, two, four, eight, and a documented high worker count on terminal, tactical, repetition-sensitive, and long-quiescence positions
- [ ] #4 A bounded CI smoke test detects hangs, duplicate or missing bestmove, illegal moves, protocol contamination, and non-terminating shutdown
- [ ] #5 A longer reproducible FastChess stress procedure completes with zero hangs, crashes, illegal moves, protocol errors, duplicate bestmoves, and time forfeits, with logs retained on failure
- [ ] #6 ThreadSanitizer or the best supported equivalent is run and documented; unsupported tooling or unavoidable false positives are recorded explicitly rather than silently skipped
- [ ] #7 Existing adversarial TT concurrency and administrative-quiescence tests remain green under the final worker lifecycle
<!-- AC:END -->
