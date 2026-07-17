---
id: TASK-1.3
title: Add the loopback UI server and `--ui` lifecycle
status: To Do
assignee: []
created_date: '2026-07-17 15:40'
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
