---
id: TASK-64.16
title: Add Lazy SMP multi-threaded search
status: To Do
assignee: []
created_date: '2026-07-19 13:34'
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
  - engine/src/options.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 79000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The search runs on one thread. Spawn multiple workers sharing the transposition table, in the Lazy SMP arrangement the codebase is already largely built for.

Existing scaffolding. The `Thread` trait with its `Master` and `Worker` types (search.rs:272-290) exists to monomorphise the search over thread role, with `Worker` never instantiated; `Master::is_master` already gates event emission (search.rs:579, :813). The transposition table is a `Box<[AtomicU64]>` with one packed eight-byte entry per slot and relaxed loads and stores (tt.rs:302-309), so readers structurally cannot observe a torn entry, and an eight-thread concurrency test already exists at tt.rs:664-688. `SearchEngine` holds the table behind an `Arc` (search.rs:120) and clones it into the worker thread.

What is missing is the worker spawn, the per-thread state split, the aggregation of results across workers, and the UCI Threads option. Per-thread state is the main design question: killers, history and the search stack are owned by `Search` and should stay per-thread, while the transposition table is shared. `Search` owns its `Position` by value, which already gives each worker an independent board.

Two constraints from existing work. TASK-15 acceptance criterion 7 requires that if a Threads option is introduced, all workers share the lock-free table defined by TASK-57, and criterion 6 requires that hash resizing and clearing happen only at an owner-controlled quiescent boundary after every worker using the old allocation has stopped. TASK-57 criteria 11 and 14 require the table to be worker-agnostic with no ownership or partitioning. Those are the contracts this task consumes.

Cancellation and time already work through shared primitives that generalise: the cancellation flag is an `AtomicBool` behind an `Arc` (search.rs:102) and the deadline is an absolute `Instant`. The completion signalling in `start_inner` (search.rs:171-194), which TASK-35 established must not rely on channel disconnection, needs extending to await all workers rather than one.

Beyond playing strength, this multiplies the throughput of NNUE self-play data generation, which is a direct constraint on how much training data is affordable. That is a separate consideration from Elo and both should be reported.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A UCI Threads option controls the number of search workers and is advertised truthfully, consistent with TASK-15
- [ ] #2 Workers share one transposition table allocation and maintain independent killers, history and search stack
- [ ] #3 Only the master thread emits search events, and the reported best move and principal variation are selected across workers by a documented rule
- [ ] #4 Completion is signalled only after every worker has finished, preserving the explicit-signal guarantee established by TASK-35
- [ ] #5 Cancellation and the time deadline stop all workers promptly
- [ ] #6 Hash resizing and clearing occur only when no worker holds the old allocation, consistent with TASK-15
- [ ] #7 Search results remain correct under thread counts from one upward, with tests covering at least one and eight workers
- [ ] #8 Both the strength gain and the change in self-play data generation throughput are measured and recorded separately
<!-- AC:END -->
