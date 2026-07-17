---
id: TASK-22
title: Make UCI output protocol clean and version consistent
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 17:15'
updated_date: '2026-07-17 23:21'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Thread a single authoritative engine identity (name, version=CARGO_PKG_VERSION, author, commit=GIT_HASH) from the seaborg binary into engine::launch, replacing hardcoded '0.0.2' strings so id name, --version, and startup metadata share one source.
2. Remove the unsolicited startup banner + 'commit:' line from protocol stdout; emit a single human diagnostic banner (with trimmed short commit) to stderr instead so no non-UCI stdout precedes the uci command.
3. Update the 'uci' handshake to emit 'id name <name> <version>' derived from the threaded identity.
4. Ensure errors/diagnostics never appear as invalid protocol messages on stdout (verify existing stderr routing; keep commit metadata on diagnostic channel).
5. Add/strengthen integration tests asserting exact startup, uci handshake, error, and readiness stdout streams; update existing tests referencing the old banner.
6. Run cargo build, cargo test, cargo fmt --check, cargo clippy.
<!-- SECTION:PLAN:END -->
