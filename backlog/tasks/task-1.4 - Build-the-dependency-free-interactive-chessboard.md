---
id: TASK-1.4
title: Build the dependency-free interactive chessboard
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-18 18:25'
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
