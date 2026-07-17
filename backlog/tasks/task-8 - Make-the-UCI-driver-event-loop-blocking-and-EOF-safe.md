---
id: TASK-8
title: Make the UCI driver event loop blocking and EOF safe
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 18:30'
labels:
  - uci
  - concurrency
dependencies:
  - TASK-1.1
references:
  - engine/src/engine.rs
priority: high
type: bug
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the typed search lifecycle lands, the UCI driver still busy-polls commands and search completion. stdin EOF repeatedly produces parse errors, while stdin read failures panic through an expect call. Replace polling and define clean command-channel, EOF, read-failure, shutdown, and active-search behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The idle UCI process blocks without continuously consuming a CPU core
- [ ] #2 Search events, search completion, and incoming commands are serviced without unbounded polling
- [ ] #3 stdin EOF, stdin read failure, or command-channel disconnection shuts the engine down cleanly without panicking or log flooding
- [ ] #4 Starting, stopping, replacing, and quitting an active search has deterministic serialized behavior
- [ ] #5 Integration tests cover EOF, stdin read failure, idle readiness, replacement search, stop, and quit
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Refactor the UCI input reader and driver behind injectable buffered input/output while preserving the public launch entry point.
2. Replace command and search busy-polling with blocking receive/select behavior, treating EOF, read failure, and channel disconnection as clean shutdown signals.
3. Serialize active-search transitions so replacement, stop, quit, and input termination cancel and join deterministically while draining typed events and formatting outcomes consistently.
4. Add integration-style driver tests for idle readiness, EOF, read failure, replacement search, stop, and quit; then run focused tests and required workspace checks.
<!-- SECTION:PLAN:END -->
