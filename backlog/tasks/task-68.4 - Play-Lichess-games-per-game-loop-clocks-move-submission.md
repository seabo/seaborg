---
id: TASK-68.4
title: 'Play Lichess games: per-game loop, clocks, move submission'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 22:34'
updated_date: '2026-07-20 10:10'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add game-stream wire types + NDJSON parsing (game_stream.rs): GameEvent enum (gameFull/gameState/chatLine/opponentGone/other), GameFull, GameState, Side, ChatLine, OpponentGone, parse_game_line. Unit tests over recorded lines.
2. Client methods: game_stream(game_id) -> iterator of GameEvent; play_move(game_id, uci) via post_empty to /api/bot/game/{id}/move/{uci}.
3. Game runner (game.rs): reconstruct Position from initialFen(or startpos)+move list; detect our side via bot account id vs white/black id; on our turn derive SearchLimit from wtime/btime/winc/binc through engine time::TimeControl with config move_overhead_ms safety margin; choose move via a MoveChooser trait (EngineMoveChooser wraps SearchEngine; tests use a deterministic first-legal-move chooser); submit UCI; stop on terminal status / no legal move. Pure search_limit fn tested directly.
4. Wire into run.rs: require_bot_account helper; run() shares Arc<LichessClient<HttpTransport>>+Arc<Config>+bot_id and, on GameStart, spawns a std::thread per game that runs play_game. run_event_loop keeps cap accounting from GameStart/GameFinish and takes a start_game callback. Update existing tests.
5. Unit coverage: full-game NDJSON fixture drives play_game end-to-end asserting legal moves posted, position sync, terminal handling; cap enforcement at event-loop level.
6. Run fmt/clippy/test.
<!-- SECTION:PLAN:END -->
