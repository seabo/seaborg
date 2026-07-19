---
id: TASK-68.4
title: 'Play Lichess games: per-game loop, clocks, move submission'
status: To Do
assignee: []
created_date: '2026-07-19 22:34'
labels: []
dependencies:
  - TASK-68.3
references:
  - 'https://lichess.org/api'
parent_task_id: TASK-68
priority: medium
type: feature
ordinal: 90000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add the actual game play on top of the TASK-68.3 scaffold. For each game the bot is in, run a per-game worker (its own thread, matching the repo's std-thread idiom) that plays the game to completion.

Scope:
- On `gameStart`, spawn a game worker that opens GET /api/bot/game/stream/{gameId} and consumes the stream: the initial `gameFull` then successive `gameState` messages (also handle `chatLine` and `opponentGone` at least minimally).
- Maintain the game's `Position` from the move list (core movegen/FEN). Detect our side and whose turn it is.
- On our turn, compute a move with the existing `SearchEngine::start(position, SearchLimit)` API. Derive the SearchLimit from the clock fields Lichess sends (wtime/btime/winc/binc) via the existing mapping in engine/src/time.rs, applying a move-time safety margin. Submit with POST /api/bot/game/{gameId}/move/{uci} using UCI move strings.
- Handle game termination cleanly (mate/resign/draw/aborted/out-of-time), including opponent-side outcomes, and free the concurrency slot so a new game can start.
- Respect the max-concurrent-games cap from config.

Out of scope: reconnect/backoff, rate-limit handling, chat commands, proactive challenges (TASK-68.5). Reuse SearchEngine; do NOT reuse GameController.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On gameStart the bot streams the game and, on its turn, submits a legal UCI move via the bot move endpoint
- [ ] #2 Search time per move is derived from the Lichess clock (wtime/btime/winc/binc) through engine/src/time.rs with a safety margin, and the bot does not lose on time under normal network conditions
- [ ] #3 The Position is reconstructed from the streamed move list and stays in sync with the server for the whole game
- [ ] #4 Games are played to completion and terminal states (win/loss/draw/abort/timeout) are handled, freeing the concurrency slot
- [ ] #5 The max-concurrent-games cap is enforced
- [ ] #6 The per-game loop has unit coverage against recorded game-stream NDJSON fixtures (no network)
- [ ] #7 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->
