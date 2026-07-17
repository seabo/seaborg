---
id: TASK-1.2
title: Add the authoritative game controller
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-17 17:38'
labels: []
dependencies:
  - TASK-1.1
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
modified_files:
  - engine/src/game.rs
  - engine/src/lib.rs
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
- [x] #1 The controller can create a game for either human side and publishes FEN, side to move, legal UCI moves, last move, move history, game status, and engine status
- [x] #2 Human moves are accepted only when legal, current, and made for the configured human side
- [x] #3 Search IDs, position revisions, and cancellation prevent stale commands or obsolete best moves from changing the active game
- [x] #4 The controller detects checkmate, stalemate, repetition, and applicable move-count draw conditions and does not search after game end
- [x] #5 Move history can be presented in SAN, including disambiguation, castling, captures, checks, checkmate, and promotion
- [x] #6 Tests cover normal play, illegal and stale commands, cancellation, undo or reset during search, castling, en passant, promotion, and terminal positions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a transport-independent GameController that owns Position, human side, revisions, move/SAN history, snapshots, terminal status, and active typed-search metadata.
2. Validate revisioned human commands against the authoritative legal-move list; implement reset and undo with search cancellation and monotonically increasing revisions/search IDs.
3. Poll typed search events/outcomes and apply a best move only when its search ID and originating revision still match the active game.
4. Add SAN generation covering ambiguity, castling, captures, checks/checkmate, en passant, and promotion, and expose immutable snapshot values including engine state.
5. Add focused controller tests for both sides, legal/illegal/stale play, lifecycle cancellation, reset/undo during search, special moves, SAN, repetition/move-count draws, mate, and stalemate.
6. Run cargo fmt --check and cargo test --workspace, record evidence, and finalize the task.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented engine::game with an authoritative single-owner GameController, owned immutable snapshots, revision-checked human commands, monotonic search IDs, cancellation on undo/reset, stale-result validation, terminal detection, SAN history, and typed engine progress state. Added 8 focused tests covering both human sides, normal/illegal/stale play, asynchronous engine turns, cancellation and stale results, undo/reset, castling, en passant, promotion, SAN disambiguation/check/mate, repetition, checkmate, stalemate, and the fifty-move threshold.

Verification so far: cargo fmt --check passes; all 8 game::tests pass; git diff --check passes. cargo test --workspace passes the core suite and all new controller tests, retaining only the two pre-existing engine failures already documented on TASK-1.1: search::tests::gives_correct_answers and tt::tests::gen_bound.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a transport-independent authoritative GameController with complete versioned snapshots, revision-validated human commands, asynchronous typed search lifecycle protection, cancellation-aware reset/undo, game-result detection, and SAN move history. Verified by 8 passing controller tests, cargo fmt --check, and git diff --check. cargo test --workspace passes all affected/new coverage and retains only the two pre-existing engine assertion failures documented on TASK-1.1.
<!-- SECTION:FINAL_SUMMARY:END -->
