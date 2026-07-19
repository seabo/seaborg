---
id: TASK-1.5
title: Complete the browser game experience and integration hardening
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-19 00:34'
labels: []
dependencies:
  - TASK-1.4
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Finish the application around the chessboard, integrate game and engine information, and verify the complete CLI-to-browser playing flow across supported interaction and lifecycle edge cases.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The responsive application lets the user start a game as White or Black, select the supported engine limit, undo or restart, flip the board, and quit the UI process
- [ ] #2 The companion panel presents SAN move history, whose turn it is, game result, engine thinking state, evaluation, depth, nodes, NPS, and principal variation without overwhelming the board
- [ ] #3 Reloading or reconnecting reconstructs the current authoritative game without duplicating a move or search
- [ ] #4 The UI gives clear recoverable feedback for rejected moves, lost connections, server errors, and occupied fixed ports
- [ ] #5 A complete game can be played through checkmate from `seaborg --ui` without console errors or an external network request
- [ ] #6 Automated and documented manual checks cover desktop and narrow layouts, both player colours, promotion, castling, en passant, terminal states, reload during search, and reduced-motion behavior
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Controller: add a runtime-settable engine search limit (applies from the next search), expose it on GameSnapshot, and derive a SAN principal variation from the searched position, truncating at the first move that is not legal.
2. Wire/server: serialize engineLimit and principalVariationSan; add POST /api/engine-limit with validated time and depth bounds, and POST /api/quit that answers before stopping the accept loop and session. Share one shutdown path between UiHandle and the quit route.
3. Frontend: extract pure presentation helpers into format.ts (score to White-relative text, node/NPS/limit formatting, human-readable command errors) so they are unit testable without a DOM.
4. Frontend app: add board flip (orientation independent of humanSide, used by rendering and keyboard navigation), restart, engine-limit select, and quit; add a companion panel rendering SAN history, turn, result, engine thinking state, evaluation, depth, nodes, NPS, hashfull and SAN principal variation.
5. Frontend feedback: readable messages for rejected moves, lost connections, server errors, and a terminal state after quit that stops reconnecting.
6. Tests: Rust unit tests for the limit command, quit, engineLimit/PV SAN serialization, reload during search not duplicating a search or move, and an HTTP-level full game to a terminal status; node --test coverage for the new pure frontend helpers; regenerate committed JS with tsc and verify it is byte-identical.
7. Docs: add a documented manual check procedure covering desktop and narrow layouts, both colours, promotion, castling, en passant, terminal states, reload during search, and reduced motion; run all repository-required checks.
<!-- SECTION:PLAN:END -->
