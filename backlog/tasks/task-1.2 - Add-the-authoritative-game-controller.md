---
id: TASK-1.2
title: Add the authoritative game controller
status: To Do
assignee: []
created_date: '2026-07-17 15:40'
labels: []
dependencies:
  - TASK-1.1
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a single-owner game session that coordinates the human side, live Position, history, legal browser commands, game results, and asynchronous engine turns. The controller is transport-independent and publishes versioned immutable snapshots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The controller can create a game for either human side and publishes FEN, side to move, legal UCI moves, last move, move history, game status, and engine status
- [ ] #2 Human moves are accepted only when legal, current, and made for the configured human side
- [ ] #3 Search IDs, position revisions, and cancellation prevent stale commands or obsolete best moves from changing the active game
- [ ] #4 The controller detects checkmate, stalemate, repetition, and applicable move-count draw conditions and does not search after game end
- [ ] #5 Move history can be presented in SAN, including disambiguation, castling, captures, checks, checkmate, and promotion
- [ ] #6 Tests cover normal play, illegal and stale commands, cancellation, undo or reset during search, castling, en passant, promotion, and terminal positions
<!-- AC:END -->
