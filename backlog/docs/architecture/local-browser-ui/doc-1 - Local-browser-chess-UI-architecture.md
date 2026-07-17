---
id: doc-1
title: Local browser chess UI architecture
type: specification
created_date: '2026-07-17 15:39'
updated_date: '2026-07-17 15:39'
tags:
  - architecture
  - ui
  - engine
---
# Local browser chess UI architecture

## Outcome

Seaborg can be launched with `seaborg --ui`, starts a loopback-only web server on an available local port, opens the default browser, and lets a person play a complete legal game against the engine through an owned dependency-free frontend.

## Constraints

- No JavaScript packages, frontend framework, bundler, CDN, analytics, or runtime network assets.
- Rust remains authoritative for position state, legal moves, special moves, and game termination.
- The existing UCI interface remains supported.
- Static HTML, CSS, JavaScript, and SVG assets are embedded in the executable.
- The server binds only to the loopback interface.

## Component model

1. The browser renders state snapshots and submits user intent. It does not implement chess rules.
2. A small local HTTP server serves embedded assets, accepts command POSTs, and streams state through Server-Sent Events.
3. A single-owner game controller owns the live Position, validates commands, tracks revisions and history, detects game results, and manages engine turns.
4. Search runs against a cloned Position and emits typed progress events and a typed result. Search never writes protocol output directly.
5. UCI and browser adapters format typed engine events for their respective transports.

## Key decisions

### In-process engine integration

The browser UI uses a typed in-process engine API rather than running Seaborg as a child UCI process. This avoids string round trips and duplicated process lifecycle management while preserving UCI as an external adapter.

### HTTP POST plus Server-Sent Events

Browser commands use narrow POST endpoints and server updates use native EventSource streaming. This supplies live search information and reconnection without a WebSocket implementation or client dependency.

### Authoritative snapshots

The server publishes versioned complete state snapshots. Commands include the position revision on which they were based. Stale search results and stale browser commands are rejected.

### Search lifecycle

Every search has a unique ID, originating position revision, and cancellation token. A best move is applied only when its ID and revision still match the active game.

## Browser experience

The responsive board supports drag-and-drop and click-click moves, mouse/touch/pen pointer events, legal target indicators, capture rings, last-move and check highlighting, promotion selection, snapback for rejected moves, board flipping, keyboard access, and reduced-motion preferences. A compact companion panel shows game controls, move history, engine state, evaluation, and principal variation.

## Local security

Bind to 127.0.0.1, validate Host and Origin, require a per-process token on mutations, cap request sizes, use a restrictive Content Security Policy, disable caching for application state, and expose no arbitrary file or general engine-command endpoints.

## Delivery order

The search refactor is the prerequisite. Subsequent delivery units add the game controller, local server and CLI lifecycle, dependency-free browser board, and final integration hardening. Exact dependencies are represented on the linked Backlog tasks.

## Verification expectations

Rust work must pass `cargo fmt --check` and `cargo test --workspace`. Controller and protocol tests cover cancellation and stale results, legal and illegal moves, castling, en passant, promotion, checkmate, stalemate, browser reload/reconnection, and loopback/security restrictions.
