---
id: TASK-1
title: Build a local browser UI for playing Seaborg
status: To Do
assignee: []
created_date: '2026-07-17 15:39'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
type: feature
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Provide an owned, fast, offline visual way to play against Seaborg without installing a chess GUI or registering the binary as a UCI engine. This parent feature tracks the staged engine, controller, transport, and frontend work described in the linked architecture specification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Running `seaborg --ui` binds a loopback-only server on an available local port and opens the application in the default browser
- [ ] #2 A user can choose a side and complete a legal game against Seaborg, including castling, en passant, promotion, checkmate, and stalemate
- [ ] #3 The browser application has no JavaScript package, framework, bundler, CDN, or runtime network dependency
- [ ] #4 The existing UCI mode continues to work after the shared engine refactor
- [ ] #5 Closing or stopping a game cannot allow an obsolete search result to mutate the active position
<!-- AC:END -->
