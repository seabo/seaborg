---
id: TASK-35
title: Fix intermittent search/UCI completion deadlock (lost search-done wakeup)
status: To Do
assignee: []
created_date: '2026-07-18 01:20'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: high
type: bug
ordinal: 38000
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
