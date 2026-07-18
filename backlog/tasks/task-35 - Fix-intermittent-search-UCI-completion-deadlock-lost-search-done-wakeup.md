---
id: TASK-35
title: Fix intermittent search/UCI completion deadlock (lost search-done wakeup)
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 01:20'
updated_date: '2026-07-18 20:13'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: high
type: bug
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Under repeated seaborg-vs-seaborg self-play (reproduced at fixed depth, concurrency>=8; far more readily on the slower debug build), the engine intermittently goes idle mid-game and never emits 'bestmove', hanging the match indefinitely. Root-caused in TASK-34 (see doc-2): the driver detects normal search completion ONLY via the SearchEvent channel becoming disconnected when the worker thread drops its Sender on exit. Thread samples at a live hang show the driver/main thread parked inside crossbeam select! on the active-search branch of next_event (engine/src/engine.rs:144-153) while the search worker thread has already exited (only main + stdin-reader threads remain) — the disconnect wakeup was lost, so finish_search never runs and no bestmove is produced. Dependency crossbeam-channel v0.5.6.

Scope: make search completion signalling robust so the driver always emits a bestmove after a search finishes. Do not rely solely on a dropped-Sender channel disconnect waking a parked select!. Candidate approaches (implementer to choose/justify): send an explicit terminal 'search complete' message before the worker returns; and/or make next_event consult SearchHandle::is_finished()/join on an explicit completion signal; and/or upgrade crossbeam-channel. This defect is independent of the illegal-PV and EOF-null-move defects and of TASK-32 (time allocation); do not change PV or time-allocation code here.

Relevant code: engine/src/engine.rs (run loop, next_event select!, finish_search/stop_search), engine/src/search.rs (SearchEngine::start worker + Sender, SearchHandle). See backlog doc-2 for full evidence and thread samples.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Under a repeated self-play stress harness (seaborg-vs-seaborg, fixed depth, concurrency>=8, both debug and release builds, at least several hundred games) the engine never hangs: every completed search emits exactly one bestmove and the match always makes progress to completion
- [ ] #2 The driver never blocks indefinitely waiting on a search whose worker thread has already finished; search completion is detected via a signal that does not depend on a lost-wakeup-prone channel disconnect
- [ ] #3 A targeted regression test exercises the search-completion / stop / replacement path (start, complete, and cancel searches in a loop) and deterministically fails on the pre-fix code or a reintroduced lost-wakeup
- [ ] #4 No changes to PV reconstruction or time-allocation code are made under this ticket; existing search-correctness and UCI tests still pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add an explicit, dedicated completion signal to the search worker: a bounded(1) `finished` channel in SearchHandle. The worker sends on it after dropping its SearchEvent Sender and before returning, so completion no longer depends on a dropped-Sender disconnect being observed.
2. Make the driver's next_event structurally incapable of blocking forever on a finished search: select over commands, search events, and the new finished signal, with a `default(poll)` arm that consults SearchHandle::is_finished(). The poll is a wakeup-independent backstop; the explicit signal keeps normal-path bestmove latency unchanged.
3. Replace DriverEvent::Search(Result<..>) with explicit SearchProgress/SearchComplete variants so every completion route converges on one finish_search call site.
4. Upgrade crossbeam-channel off the pinned 0.5.6 implicated in doc-2, as defence in depth (not the primary fix).
5. Regression test: add a test-only SearchEngine hook that retains a clone of the worker's event Sender so the events channel can never disconnect, then drive next_event over a start/complete/cancel loop under a watchdog thread with recv_timeout. This deterministically hangs (fails) on the pre-fix code and on any reintroduced disconnect-only dependency.
6. Validate with a self-play stress harness (fastchess, seaborg-vs-seaborg, fixed depth, concurrency>=8) on both debug and release builds, several hundred games, and record evidence.
7. No PV or time-allocation code is touched.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the explicit completion signal and made the driver's wait structurally unable to block forever on a finished search.

search.rs: SearchHandle gains a bounded(1) 'finished' channel. The worker sends on it after dropping its event Sender and before returning, for both completed and cancelled searches. start() is now a thin wrapper over start_inner(), which also hands back a clone of the worker's event Sender; the #[cfg(test)] start_retaining_events() exposes that clone so tests can pin the events channel open.

engine.rs: DriverEvent::Search(Result<..>) is replaced by explicit SearchProgress/SearchComplete variants, so all three completion routes converge on one finish_search call site. next_event now selects over commands, events, and the finished signal, with a default(50ms) arm that consults SearchHandle::is_finished() directly. The disconnect path is retained as one route of three rather than the only one. The poll is a backstop, not the normal path: the explicit signal keeps bestmove latency unchanged, and the poll bounds the cost of a lost wakeup at 50ms instead of forever.

Note the same poll-based approach is already used by the other consumer of SearchHandle, game.rs::poll (try_iter + is_finished), which is why that driver never had this exposure and is untouched here.

crossbeam-channel 0.5.6 -> 0.5.16 as defence in depth. Deliberately not the primary fix: the poll backstop holds regardless of channel implementation.

AC #4: no PV or time-allocation code touched. The diff is limited to engine.rs, search.rs, engine/Cargo.toml and Cargo.lock.

STRESS TESTING FOUND A SECOND, PRE-EXISTING DEFECT -> TASK-54.

Debug-build self-play with a mate-rich opening book wedges permanently, on this branch AND on unmodified master (5b592eb). Root-caused from FastChess raw protocol logs (-log ... engine=true): debug_assert!(plies_to_mate % 2 == 0) at engine/src/score.rs:179 panics the driver/main thread while formatting a mate score ('assertion failed: plies_to_mate % 2 == 0', all 8 wedged slots). The panic unwinds out of the run loop and thread::scope then blocks forever joining the scoped stdin reader parked in read_line, so the process hangs instead of dying.

That is why the thread sample shows main parked under thread::scope with the reader in read_line -- a signature easily mistaken for a completion deadlock, and distinct from doc-2's signature (main parked in crossbeam select! with the worker exited), which is the defect fixed here.

It also resolves doc-2's asymmetry that 'the debug build hangs readily' while release did not: debug_assert! is compiled out in release. Some of the debug hangs attributed to the lost-wakeup defect may in fact have been this panic. score.rs is untouched by this task's diff.

Consequence: AC #1 is only partially evidenced. Its release half passes strongly; its debug half cannot pass until TASK-54 is fixed, and not because of anything in this change. Filed as TASK-54 per user decision rather than widening this task's scope.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 12:03
---
Coordination with TASK-39 (recorded by the TASK-34 rework).

TASK-39 investigates UCI 'stop' responsiveness under TASK-32's abort-suppressed ply-1 window. Both tickets touch the stop/abort path, so scope the boundary explicitly to avoid duplicate work:

- TASK-35 (this ticket) is about the driver never being NOTIFIED that a search finished: a lost channel-disconnect wakeup leaves the driver parked in select! while the worker has already exited, so no bestmove is ever emitted. It is a completion-signalling defect in engine/src/engine.rs.
- TASK-39 is about how QUICKLY an in-flight search honours a stop request during ply 1. It is a responsiveness question in engine/src/search.rs (stopping() / min_search_complete).

They are independent: TASK-35's hang occurs after the search has completed and its worker has exited, so the suppression window plays no part in it, and fixing the completion signal does not change stop latency. Neither fix should need to touch the other's code. If an implementer finds they do interact, stop and reconcile the two tickets rather than widening either.

Also note: TASK-35's ordinal moved 38000 -> 40000 to clear a collision with TASK-38 filed on master.
---

author: @codex
created: 2026-07-18 20:13
---
Stress-testing this fix uncovered a second, pre-existing hang that is NOT this ticket's defect and is NOT caused by this change: a debug_assert in Score's Display impl (engine/src/score.rs:179) panics the driver thread on a mate score, and thread::scope then blocks process exit on the parked stdin reader. It reproduces identically on unmodified master. Filed as TASK-54 with full evidence.

This bounds what AC #1 can claim here. Release-build evidence is strong (400 games, mate-rich book, 27908/27908 searches answered). The debug half of AC #1 is blocked by TASK-54, so I have left AC #1 unchecked for the reviewer rather than checking it on partial evidence. Reviewer: please treat AC #1 as blocked-by-TASK-54, not as satisfied.
---
<!-- COMMENTS:END -->
