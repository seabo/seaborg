---
id: TASK-64.16.5
title: Make UCI progress and completion team-aware
status: To Do
assignee: []
created_date: '2026-07-19 23:24'
labels:
  - uci
  - search
  - concurrency
  - telemetry
dependencies:
  - TASK-64.16.4
references:
  - engine/src/search.rs
  - engine/src/trace.rs
  - engine/src/info.rs
  - engine/src/engine.rs
  - engine/src/game.rs
  - engine/src/ui/wire.rs
parent_task_id: TASK-64.16
priority: high
type: enhancement
ordinal: 96000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make all externally visible search telemetry describe the complete Lazy SMP team while keeping the official score and PV tied to a coherent completed master result. The current SearchProgress is emitted from one local Tracer and the driver drains one event stream before bestmove; after SMP, helper work must be included in nodes and NPS without allowing helper-local partial state or stale events to corrupt the master report.

Progress reporting must be cheap enough not to become a scaling bottleneck. Stale telemetry may be coalesced or dropped, but terminal completion and the final official result may not be lost. Events from a cancelled or replaced search must remain distinguishable from its successor.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 UCI info nodes and nps include work from every worker and use one team elapsed-time origin
- [ ] #2 UCI score, depth, seldepth if added, PV, currmove, and bestmove remain internally coherent and belong to the documented official completed result
- [ ] #3 hashfull reports the shared table and is not multiplied or averaged across workers
- [ ] #4 Helper progress publication does not create a contended per-node hot path or an unbounded backlog of stale telemetry
- [ ] #5 The final master event backlog is drained before the single bestmove line, and completion cannot overtake the official result
- [ ] #6 Search identity prevents events from a stopped or replaced team from being attributed to its successor in UCI, game-controller, or browser output
- [ ] #7 Transcript and controller tests cover natural completion, stop, replacement go, setoption, ucinewgame, EOF, quit, and rapid consecutive searches with multiple workers
<!-- AC:END -->
