---
id: TASK-64.16.4
title: Deliver correctness-first homogeneous Lazy SMP
status: To Do
assignee: []
created_date: '2026-07-19 23:24'
updated_date: '2026-07-19 23:24'
labels:
  - search
  - concurrency
  - uci
  - strength
dependencies:
  - TASK-15
  - TASK-64.3
  - TASK-64.18
  - TASK-64.19
  - TASK-64.16.2
  - TASK-64.16.3
references:
  - engine/src/search.rs
  - engine/src/tt.rs
  - engine/src/options.rs
  - engine/src/engine.rs
  - engine/src/uci.rs
  - engine/src/game.rs
parent_task_id: TASK-64.16
priority: high
type: feature
ordinal: 95000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Enable the first production multi-worker search. One master and Threads minus one helpers independently run the existing iterative-deepening alpha-beta search from cloned root positions while sharing the single clustered transposition table. Mutable board, PV, evaluation, search stack, ordering state, killers, history, and tracing remain private to each worker.

This baseline intentionally uses homogeneous workers and a conservative result policy. Only the master emits progress and its last completed iteration is authoritative; helpers contribute by populating and consuming the shared TT. This separates the correctness and lifecycle milestone from later diversification and result-voting experiments.

Threads=1 is a compatibility contract, not merely a supported value: it must preserve the pre-SMP search path, node-limited reproducibility, result, and protocol behavior except for changes separately justified and measured in this task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The validated UCI Threads option controls one master plus the requested number of total workers and is advertised truthfully with documented default and bounds
- [ ] #2 Every worker searches an independent position with independent mutable heuristics and search state while sharing exactly one Arc<Table> allocation
- [ ] #3 Only the master emits search events, and the master last completed iteration supplies the official score, PV, depth, and best move
- [ ] #4 Helpers can improve another worker through TT entries, demonstrated by a deterministic test rather than timing alone
- [ ] #5 Fixed-depth completion, time, nodes, infinite, stop, replacement go, setoption, ucinewgame, quit, EOF, and terminal roots work correctly with one and eight workers
- [ ] #6 Threads=1 matches the recorded pre-SMP result and node count on a representative deterministic fixed-depth and node-limited corpus
- [ ] #7 All reported best moves are legal, aborted iterations never become official, and a cancelled team retains the legal-root-fallback guarantee
- [ ] #8 Repository checks and a bounded multi-worker FastChess smoke run complete with no hang, crash, protocol error, illegal move, duplicate bestmove, or time forfeit
<!-- AC:END -->
