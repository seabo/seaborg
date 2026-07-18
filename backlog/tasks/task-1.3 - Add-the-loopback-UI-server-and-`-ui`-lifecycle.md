---
id: TASK-1.3
title: Add the loopback UI server and `--ui` lifecycle
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-18 11:57'
labels: []
dependencies:
  - TASK-1.2
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Host the game controller through a deliberately narrow local HTTP interface, serve embedded application assets, stream snapshots and search information, and integrate startup and shutdown with the Seaborg CLI.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `seaborg --ui` binds to 127.0.0.1 on an available port, prints the URL, and opens it only after the listener is ready
- [ ] #2 `--ui-port` selects a fixed port and `--no-open` suppresses browser launch, with clear errors for bind or launch failures
- [ ] #3 Embedded application assets and current state are available over GET, commands use bounded POST endpoints, and updates stream through a reconnectable Server-Sent Events endpoint
- [ ] #4 Mutating requests require the process session token and unexpected Host or Origin values are rejected
- [ ] #5 Responses set appropriate content types, no-store state caching, and a restrictive Content Security Policy
- [ ] #6 `--ui`, `--uci`, and `--dev` cannot be selected together
- [ ] #7 Protocol tests cover startup, state retrieval, command validation, SSE reconnection, request limits, and shutdown
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine::ui module: hand-rolled loopback HTTP/1.1 server on std::net::TcpListener (no new dependencies), thread-per-connection, with a small owned JSON writer for snapshot serialization.
2. Bind 127.0.0.1 on port 0 (or --ui-port), resolve the actual local port from the listener, print the URL, and only then launch the browser via the existing open crate; --no-open suppresses launch. Report bind and launch failures with clear errors.
3. Route a narrow fixed surface: GET / and static embedded assets (include_str!/include_bytes! placeholder index.html, app.js, style.css), GET state snapshot JSON, bounded POST command endpoints (move, undo, reset, new game), and a reconnectable SSE stream carrying versioned snapshots and search progress.
4. Own a single GameController behind a Mutex on a dedicated driver thread that calls poll() on a cadence and publishes revisions to SSE subscribers; SSE clients reconnect by last-seen revision and immediately receive current state.
5. Enforce local security: per-process random session token required on all mutating requests, Host and Origin allowlist restricted to loopback, capped request line/header/body sizes, restrictive Content-Security-Policy, no-store on state responses, correct content types, and no arbitrary file or generic engine-command endpoint.
6. Wire the CLI: add --ui, --ui-port, --no-open to clap Args and make --ui, --uci, --dev mutually exclusive; integrate startup and graceful shutdown.
7. Add protocol tests driving the server over real loopback sockets: startup and port selection, asset and state retrieval, command validation (illegal, stale revision, missing/incorrect token, bad Host/Origin, oversized body), SSE streaming and reconnection, request limits, mutual-exclusion flag errors, and shutdown.
8. Run cargo fmt --check and cargo test --workspace, recording any pre-existing baseline failure, then commit the implementation target and hand off to review.
<!-- SECTION:PLAN:END -->
