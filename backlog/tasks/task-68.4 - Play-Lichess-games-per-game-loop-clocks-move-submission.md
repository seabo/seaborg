---
id: TASK-68.4
title: 'Play Lichess games: per-game loop, clocks, move submission'
status: In Review
assignee:
  - '@george'
created_date: '2026-07-19 22:34'
updated_date: '2026-07-20 10:24'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented per-game play on the TASK-68.3 scaffold.

New/changed code (lichess crate):
- game_stream.rs (new): wire types + parse_game_line for the bot game stream
  (gameFull/gameState/chatLine/opponentGone/other). GameState::is_ongoing()
  distinguishes created/started from terminal statuses. GamePlayer.id is
  optional so an AI side (no id) is never mistaken for the bot.
- client.rs: game_stream(game_id) yields GameEvent per line; play_move(game_id,
  uci) POSTs /api/bot/game/{id}/move/{uci}. Both reuse the existing Transport
  trait unchanged (open_stream + post_empty), so the fake-transport test path
  from 68.3 extends to game play with no network.
- game.rs (replaces the GameHandoff seam): play_game() opens the stream and,
  per state, replays the server's move list from the initial position (startpos
  or initialFen) into a core Position, finds the bot's side by account id, and
  on the bot's turn derives the search budget and submits the chosen move.
  Move choice is behind MoveChooser; EngineMoveChooser wraps a per-game
  SearchEngine (TT persists across the game). Terminal status stops the loop;
  no legal move (mate/stalemate) submits nothing and waits for the terminal
  state.
- run.rs: require_bot_account() replaces serve(); run() shares
  Arc<LichessClient<HttpTransport>> + Arc<Config> and, on gameStart, spawns a
  std::thread worker running play_game. run_event_loop() gained a start_game
  callback and still keeps the active-game count from the account stream's
  gameStart/gameFinish events to enforce the cap.

Design decisions:
- Search budget (time.rs): the configured move_overhead_ms is held back from
  the bot's clock before TimeControl slices it, rather than trimmed off the
  final allotment. Reducing the pool keeps the allocation proportional at fast
  controls (a flat post-hoc deduction would collapse fast-control budgets to
  zero, the pathology time.rs already guards against). moves_to_go is None:
  Lichess real-time games have no periodic control.
- Position is rebuilt from the authoritative move list every state (not tracked
  incrementally), so any divergence surfaces as an explicit Decode error and
  the bot cannot silently desync.
- Concurrency cap stays sourced from the account event stream's game lifecycle
  events, so a lagging worker cannot miscount the cap.

Reconnect/backoff, rate limits, chat commands, opponent-gone win claims, and
proactive challenges are out of scope (TASK-68.5) and not implemented.

Test coverage: game_stream parse tests (players/clocks/status/AI/chat/gone/
unknown/malformed); game-loop tests over recorded NDJSON fixtures (Scholar's
mate with the bot as black asserting a legal move on each of three turns and
position sync; immediate white-side move; missing initialFen; chat/gone cause
no move; gameState-before-gameFull, non-participant, and illegal-stream-move
errors); search-budget derivation tests (overhead held back, stays under the
clock, larger margin never increases the budget, tiny clock saturates to zero).
run.rs event-loop tests updated to assert gameStart hands the game to the
runner while the cap still gates challenges.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-20 10:24
---
Implementation handoff
Branch: task-68.4-lichess-game-play
Worktree: /Users/seabo/seaborg-worktrees/task-68.4-lichess-game-play
Base: f84b6d8c6afd11c30841cf287a38fa82daacd648
Implementation target: 617a4b5
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (lichess 50, engine 272 [2 ignored], core 45, plus workspace suites; 0 failures)
Known failures: none
---
<!-- COMMENTS:END -->
