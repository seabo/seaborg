---
id: TASK-1
title: Build a local browser UI for playing Seaborg
status: Done
assignee: []
created_date: '2026-07-17 15:39'
updated_date: '2026-07-19 14:20'
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
- [x] #1 Running `seaborg --ui` binds a loopback-only server on an available local port and opens the application in the default browser
- [x] #2 A user can choose a side and complete a legal game against Seaborg, including castling, en passant, promotion, checkmate, and stalemate
- [x] #3 The browser application has no JavaScript package, framework, bundler, CDN, or runtime network dependency
- [x] #4 The existing UCI mode continues to work after the shared engine refactor
- [x] #5 Closing or stopping a game cannot allow an obsolete search result to mutate the active position
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Parent-level finalization. All five subtasks (TASK-1.1 through TASK-1.5) were implemented, independently reviewed and merged to master; the browser-level manual checks in docs/browser-ui-manual-checks.md were exercised and reviewed under TASK-1.5 (real-browser run at 1440x1000 and 390x844, no console errors, 12 same-origin resources and no off-origin requests, keyboard-only move played end to end).

Verified at master aec9992: cargo test --workspace 235 passed / 0 failed (2 ignored) plus 5 build-metadata tests and 1 doc-test; cargo fmt --check clean; tsc -p engine/src/ui/frontend/tsconfig.json left engine/src/ui/assets byte-identical, so the embedded JS matches its TypeScript source; node --test board.test.mjs format.test.mjs 16/16 passed.

AC evidence: #1 TcpListener::bind((Ipv4Addr::LOCALHOST, port)) with port.unwrap_or(0) at engine/src/ui/server.rs:276, browser launch via the open crate at :375, plus non-loopback peer rejection at :388 and Host/Origin checks at :467. #2 legality delegated to core movegen at engine/src/game.rs:135, promotion chooser at assets/index.html:119, checkmate/stalemate at game.rs:348-355, driven end to end over HTTP in engine/src/ui/tests.rs:1400. #3 no package.json, node_modules or lockfile; assets embedded with include_str! at server.rs:24; the only script tag is same-origin /app.js and all JS imports are relative. #4 --uci still wired at src/cmdline.rs:52; UI and UCI are peers on the same typed SearchEngine API; 16 driver tests plus 6 parser tests. #5 revision counter plus re-validation of the move against the current position at game.rs:215-221, cancel_search cancels and joins, covered by stale_or_cancelled_search_outcomes_are_never_applied at game.rs:550.

Also removed the 13 'Covers AC #N' annotations from docs/browser-ui-manual-checks.md. They tracked TASK-1.5's criteria rather than this parent's (they referenced an AC #6 that TASK-1 does not have), so they were misleading as finalization evidence and hurt readability.

Observation, not a defect: the final conjunct 'id < self.next_search_id' at game.rs:219 is tautological because next_search_id is incremented past id at :292 on creation. It is harmless redundancy, but reads as load-bearing protection when the actual guard is 'revision == self.revision'. Not addressed here; no follow-up task created.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Delivered the offline local browser UI for playing Seaborg across five subtasks: a reusable typed engine API shared by UCI and the UI, an authoritative game controller, a loopback-only UI server with the --ui lifecycle, a dependency-free interactive chessboard, and the completed browser game experience. seaborg --ui binds 127.0.0.1 on an OS-assigned port and opens the default browser; a full legal game including castling, en passant, promotion, checkmate and stalemate can be played from the browser with no JavaScript package, framework, bundler, CDN or runtime network dependency; UCI mode is unchanged; and stale search results cannot mutate the active position. Verified at master aec9992 by cargo test --workspace (235 passed, 0 failed), cargo fmt --check, a tsc rebuild leaving the committed assets byte-identical, node --test frontend suites (16/16), and the real-browser manual checks exercised and independently reviewed under TASK-1.5.
<!-- SECTION:FINAL_SUMMARY:END -->
