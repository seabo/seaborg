---
id: TASK-22
title: Make UCI output protocol clean and version consistent
status: To Do
assignee: []
created_date: '2026-07-17 17:15'
labels:
  - uci
  - release
dependencies:
  - TASK-1.1
references:
  - engine/src/engine.rs
  - src/main.rs
  - Cargo.toml
priority: medium
type: bug
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The process emits unsolicited startup text and several diagnostics on protocol stdout, while Cargo metadata and the engine banner report different versions. Ensure GUI-facing stdout contains only valid UCI traffic and derive one consistent engine identity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Starting UCI mode emits no unsolicited non-UCI stdout before the uci command
- [ ] #2 Errors and optional human diagnostics do not appear as invalid protocol messages on stdout
- [ ] #3 The id name response, command-line version, and startup metadata derive from one authoritative package version
- [ ] #4 Commit metadata is trimmed and, when shown, is emitted through an appropriate diagnostic channel or UCI info form
- [ ] #5 Integration tests assert the exact startup, uci handshake, error, and readiness output streams
<!-- AC:END -->
