---
id: TASK-64.16
title: Build a robust and performance-tuned Lazy SMP search system
status: To Do
assignee: []
created_date: '2026-07-19 13:34'
updated_date: '2026-07-19 23:22'
labels:
  - search
  - concurrency
  - nnue
  - performance
dependencies:
  - TASK-57
  - TASK-15
  - TASK-64.1
references:
  - engine/src/search.rs
  - engine/src/tt.rs
  - engine/src/engine.rs
  - engine/src/options.rs
  - engine/src/info.rs
  - engine/src/trace.rs
  - tools/strength/strength_test.py
  - docs/strength-testing.md
parent_task_id: TASK-64
priority: high
type: feature
ordinal: 79000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Programme umbrella for turning the single-worker asynchronous search into a production-quality Lazy SMP system. Multiple independent search instances will search cloned copies of the same root position, share the lock-free clustered transposition table, and influence one another through verified TT entries. The programme covers lifecycle ownership, limits and telemetry, UCI configuration, correctness hardening, worker diversification, optional result voting, scaling analysis, and strength tuning.

Current state. SearchEngine::start spawns exactly one background thread and calls Search::run<Master>. Worker exists only as a reporting-role marker and has no production caller. Search-local Position, evaluation state, PV table, per-ply stack, killers, history, ordering state, and Tracer are already natural per-worker state. Cancellation and the deadline are shareable, SearchHandle cancels and joins on drop, and TASK-57 delivered one worker-agnostic Arc<Table> with bounded lock-free probes and stores plus concurrency tests.

Programme principles. First preserve the exact one-thread behavior and establish a correctness-first homogeneous Lazy SMP baseline in which the master result is authoritative. Then add deterministic diversification, and evaluate cross-worker result voting separately so speculative strength policy cannot compromise lifecycle correctness. Aggregate node accounting must avoid a contended atomic increment on every node. Administrative hash operations and resource changes remain quiescent owner operations after every worker has joined.

This is an umbrella and carries no direct implementation. Its child tasks are independently reviewable delivery points. Strength-changing policies must be measured one at a time against the immediately preceding accepted build with the repository strength harness.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every child task is Done or explicitly closed with a recorded decision and evidence
- [ ] #2 Threads=1 preserves the single-worker search behavior, lifecycle guarantees, legal-move fallback, completed-iteration semantics, and UCI protocol behavior
- [ ] #3 Configured multi-worker searches share one transposition-table allocation while keeping all mutable search heuristics and board state per worker
- [ ] #4 Cancellation, completion, resource changes, panic handling, and drop paths cannot detach or outlive any member of a search team
- [ ] #5 Time and node limits plus UCI nodes, nps, time, hashfull, score, PV, and bestmove have documented team-wide semantics
- [ ] #6 The final retained diversification and result-selection policies are supported by correctness tests, scaling measurements, and statistically meaningful strength evidence
- [ ] #7 A closing report records 1, 2, 4, and 8 worker scaling, Elo, time-loss and illegal-move outcomes, and self-play data-generation throughput separately
<!-- AC:END -->
