---
id: TASK-1.5
title: Complete the browser game experience and integration hardening
status: To Do
assignee: []
created_date: '2026-07-17 15:40'
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
