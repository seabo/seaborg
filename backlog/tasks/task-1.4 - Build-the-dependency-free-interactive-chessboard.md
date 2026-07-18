---
id: TASK-1.4
title: Build the dependency-free interactive chessboard
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-18 19:59'
labels: []
dependencies:
  - TASK-1.3
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create the owned HTML, CSS, TypeScript, and SVG board experience that renders authoritative controller snapshots and turns mouse, touch, pen, click, and keyboard interaction into narrow move commands. Author the web app logic in TypeScript and compile it to locally served JavaScript for the browser.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The board renders every FEN position correctly in either orientation using locally bundled assets
- [ ] #2 Users can move by drag-and-drop or click-click with mouse, touch, and pen input
- [ ] #3 Selection, legal destinations, captures, the previous move, check, rejected-move snapback, and engine-thinking lockout have clear visual states
- [ ] #4 Castling and en passant animate correctly and promotion presents an accessible queen, rook, bishop, or knight chooser
- [ ] #5 The board is responsive, keyboard operable, labelled for assistive technology, and respects reduced-motion preferences
- [ ] #6 The web app source is TypeScript compiled to locally served JavaScript, and the client runtime uses no third-party JavaScript, framework, bundler, CDN, font service, or runtime network asset
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extend authoritative game snapshots and browser JSON with an explicit in-check flag, with Rust coverage, so the client can highlight check without implementing chess rules.
2. Replace the placeholder page with a responsive semantic board shell and a locally embedded SVG piece sprite; add fixed server routes and protocol tests for every shipped asset.
3. Implement strict TypeScript board/model modules compiled to committed ES modules: FEN rendering in both orientations, legal-target/capture derivation, pointer drag and click-click input, roving keyboard controls, promotion dialog, lockout, status highlights, and transition metadata for ordinary moves, captures, castling, en passant, and rejected-move snapback.
4. Add focused dependency-free frontend tests for FEN parsing, orientation, move selection/promotion, capture classification, and special-move transitions; verify the generated JavaScript matches the TypeScript build.
5. Exercise the board in a real local browser at desktop and narrow sizes, including both orientations, keyboard/pointer flows, special moves, check, rejection, engine lockout, and reduced-motion behavior; then run all repository-required Rust checks and prepare the immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the dependency-free browser board as strict TypeScript compiled to committed ES modules, with a semantic responsive HTML shell, owned CSS, and a locally embedded twelve-piece SVG sprite. The client parses only FEN placement, while Rust remains authoritative for legal moves and now publishes an explicit inCheck flag. Pointer Events provide click-click and drag input across mouse, touch, and pen; a roving grid focus model provides arrow-key and Enter/Space operation. Legal target dots, capture rings, selection, previous-move and check highlights, engine/command lockout, persistent rejection feedback, and snapback are represented as explicit states. Promotion uses a modal four-piece chooser. Snapshot diffs drive ordinary, capture, castling-rook, and en-passant capture animations, with a stale-revision guard for POST/SSE races and a reduced-motion override.

Frontend verification: strict tsc passed; a clean temporary compilation matched both committed JavaScript files byte-for-byte; node --test passed 7 model tests covering FENs/all piece kinds, both orientations, keyboard coordinate mapping, ordinary/en-passant targets, all promotion choices, both castling directions, en-passant transitions, and assistive labels. A real Chrome run against the built loopback binary rendered 64 squares/32 pieces at 1440x1000 and 390x844 with no overflow, console/CSP error, or external resource; it applied click-click, keyboard, mouse-drag, pen-drag, and touch-drag moves, reconstructed Black orientation, showed invalid-drop snapback and live engine-thinking lockout, and reduced animation duration to 0.001ms under emulated prefers-reduced-motion.

Rust verification: cargo fmt --check passed; cargo clippy --workspace --all-targets --all-features -- -D warnings passed; cargo test --workspace passed 200 tests with 0 failures and 1 ignored; cargo build passed. No move-generation or search hot path changed, so benchmarks were not applicable.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 19:59
---
Implementation handoff
Branch: task-1.4-interactive-chessboard
Worktree: /Users/seabo/seaborg-worktrees/task-1.4-interactive-chessboard
Base: 9c4cc1896a6f9b121412bb9c5cf86b338891367d
Implementation target: 9319c0d27963a721fd1c04c3d02e8cb2e8f56eb0
Resolved findings: none
Verification:
- tsc -p engine/src/ui/frontend/tsconfig.json: passed
- clean temporary tsc build compared with committed app.js and board.js: byte-identical
- node --test engine/src/ui/frontend/board.test.mjs: passed, 7/7
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed, zero warnings
- cargo test --workspace: passed, 200 tests, 0 failures, 1 ignored
- cargo build: passed
- Chrome against target/debug/seaborg --ui: desktop 1440x1000 and narrow 390x844 rendered without overflow; click-click, keyboard, mouse/pen/touch drag, both orientations, snapback, engine lockout, and reduced motion passed; no browser console/CSP errors or external resources
Known failures: none
---
<!-- COMMENTS:END -->
