---
id: TASK-1.5
title: Complete the browser game experience and integration hardening
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-19 01:05'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Controller and protocol:
- GameController gained a runtime-settable search limit that applies from the next engine turn (a running search keeps the limit it started with, so adjusting a setting never discards work or changes the move about to be played). The limit is published on GameSnapshot as engineLimit.
- Thinking snapshots now carry principalVariationSan beside SearchProgress. SAN is derived in the controller because only it holds the position the reported moves are read against; the line is truncated at the first move that is not legal there, since a transposition-table move can survive into a line it cannot be played in.
- New endpoints: POST /api/engine-limit (kind time|depth with a bounded value; Infinite is deliberately unreachable because it would never produce a reply) and POST /api/quit (answers 200 before stopping, so the browser sees acceptance rather than a dropped connection). UiHandle and the quit route now share one ShutdownSignal instead of open-coding the stop sequence twice.
- format.js is a sixth embedded asset.

Browser:
- Companion panel showing SAN move history as a numbered scoresheet, turn, result, engine idle/thinking, evaluation, depth, nodes, NPS, hash, and the SAN principal variation. Evaluation is normalised to White, since the engine scores relative to the side it searches for.
- Controls: new game as White or Black, restart (a new game on the side already being played, so no new command was needed), undo, flip board, a thinking-limit select, and quit.
- Recoverable feedback: server codes are rendered as sentences, 5xx is distinguished from a rejection, connection loss is announced and cleared on recovery, and quit puts the page into a terminal state that stops reconnecting.
- Pure presentation rules were extracted into format.ts so they are unit testable without a DOM.

Two defects were found by driving the real page in Chrome and are fixed here:
- render() returned early when a newer revision had already arrived, skipping the whole frame rather than just the stale state. The pending-command flag therefore cleared without being painted, leaving the status on 'Sending move…' with controls disabled until an unrelated event repainted. Now the stale snapshot is still not adopted but the frame is always painted (shouldAdopt, with a regression test).
- Repainting replaces every square and the replacements are disabled while the engine thinks, so a keyboard user was dropped onto the document on selecting a piece and again on every engine turn. The board now tracks that it owns the keyboard and takes focus back once a square can hold it.

Out of scope, observed and confirmed pre-existing: the engine misses a mate in one (1.f3 a6 2.g4 missing 2...Qh4#). Reproduced through plain --uci, which uses none of this task's code, so it is a search issue rather than a UI one. Not fixed here.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 01:05
---
Implementation handoff
Branch: task-1.5-browser-game-experience
Worktree: /Users/seabo/seaborg-worktrees/task-1.5-browser-game-experience
Base: 4d48c35917a2955550f5a0bbc6a0120d3b0cc957
Implementation target: <handoff commit>
Resolved findings: none (first implementation attempt)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 225 passed / 0 failed / 2 ignored (the 2 ignored are pre-existing)
- tsc -p engine/src/ui/frontend/tsconfig.json then git diff --exit-code engine/src/ui/assets: pass, regenerated JS byte-identical to the committed output
- node --test board.test.mjs format.test.mjs: pass, 15 passed / 0 failed
- Real-browser run of seaborg --ui at 1440x1000 and 390x844: board square with 64 equal squares and no horizontal overflow, no console errors, and no off-origin requests (12 resources, all same-origin). Keyboard-only move played end to end with focus retained across the engine turn. Flip, restart, undo, limit select, and new game as either colour all exercised.
- Complete game to checkmate through the running server: 60 plies, final gameStatus {kind: checkmate, winner: black}, 0 legal moves, and a further move refused 409 game_over.
- Quit against the real process: 200 {quitting:true}, listener stopped, 'Seaborg UI stopped.' printed, exit status 0.
- Occupied fixed port: second instance on the same port exits 1 naming the port and suggesting --ui-port.
Known failures: none.

Note for review: the engine misses a mate in one (1.f3 a6 2.g4 missing 2...Qh4#). I confirmed this reproduces through plain --uci, which uses none of this task's code, so it is pre-existing search behaviour and is not addressed here.
---

author: @claude
created: 2026-07-19 01:05
---
Correction to the handoff above: the implementation target is 29fa503b, this task-only handoff commit. The tree at 29fa503b is identical to f3052af except for the Backlog task file, so every verification result recorded above holds at the target.
---

author: @claude
created: 2026-07-19 01:05
---
Disregard the two sha corrections above; they chased their own commits. Settling it:

Implementation target: f3052af30e8f2db2c7e2264d8ba96f154f775c26 — the last commit containing implementation changes, and the commit every verification result in the handoff was measured at.

Every commit after f3052af on this branch touches only backlog/tasks/task-1.5, which the following verifies:
  git diff --stat f3052af..HEAD
Review the branch diff 4d48c35..f3052af for the implementation.
---
<!-- COMMENTS:END -->
