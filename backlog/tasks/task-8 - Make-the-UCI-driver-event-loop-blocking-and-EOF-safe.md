---
id: TASK-8
title: Make the UCI driver event loop blocking and EOF safe
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 18:37'
labels:
  - uci
  - concurrency
dependencies:
  - TASK-1.1
references:
  - engine/src/engine.rs
modified_files:
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Replaced the UCI busy-poll loop with blocking command receives while idle and crossbeam selection between commands and typed search events while active. EOF, read errors, and command-channel disconnection now converge on clean shutdown. Stop, replacement go, quit, and input termination synchronously cancel and join the active search before the next transition. Injected I/O enables driver-level coverage for EOF, read failure, idle readiness, natural completion, replacement, stop, and quit.

Verification: cargo fmt --check passed; all 4 focused engine::tests passed; git diff --check passed. cargo test --workspace --no-fail-fast completed with 40 tests passing, 1 ignored, and the documented baseline failure tt::tests::gen_bound (assertion gen < 64).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 18:37
---
Implementation handoff
Branch: task-8-uci-blocking-eof-safe
Worktree: /Users/seabo/seaborg-worktrees/task-8-uci-blocking-eof-safe
Base: 46aa66ed04ba91d9526114dd01c5f06fe5668ee4
Implementation target: 1a3f4be19ea783038905d954c153927bd7f303d1
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo test -p engine engine::tests: 4 passed
- git diff --check: passed
- cargo test --workspace --no-fail-fast: 40 passed, 1 ignored; one known baseline failure
Known failures: tt::tests::gen_bound asserts gen < 64, previously documented and reproduced on master during TASK-1.1 review.
---
<!-- COMMENTS:END -->
