---
id: TASK-68
title: Lichess bot integration
status: Done
assignee: []
created_date: '2026-07-19 22:32'
updated_date: '2026-07-20 17:52'
labels: []
dependencies: []
references:
  - 'https://lichess.org/api'
  - 'https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml'
priority: medium
type: feature
ordinal: 86000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let Seaborg connect to a Lichess BOT account and play games autonomously via the Lichess Bot API, driven by a new `seaborg lichess` subcommand.

This is the umbrella task; the concrete work is split across subtasks. Architecture decided up front (do not re-litigate during implementation):

- HTTP transport is synchronous and thread-per-stream using `ureq` (rustls TLS). No async runtime (no tokio/reqwest) — this matches the existing std-thread + crossbeam-channel idiom used by the browser UI server.
- JSON is handled with `serde` + `serde_json` (scoped where added; see the serde-adoption subtask). Inbound Lichess schemas are non-trivial and externally controlled, so hand-rolling is out.
- Lives in a new `lichess` workspace crate depending on `engine` (for the `SearchEngine` API in engine/src/search.rs) and `core` (Position/movegen/FEN). Keeps networking/TLS/serde out of the lean UCI binary path.
- Reuse `SearchEngine::start(position, SearchLimit)` and the clock->SearchLimit mapping in engine/src/time.rs. Do NOT reuse `GameController` (it is modeled for local human-vs-engine); build a purpose-built per-game loop instead.
- Auth: an API token with the `bot:play` scope (plus `challenge:write` only if proactive challenges are added), supplied via the `LICHESS_BOT_TOKEN` env var. Acceptance policy and engine settings come from a TOML config file.
- The account must be upgraded to a BOT account via POST /api/bot/account/upgrade, which is irreversible and only works on an account with zero games played — gated behind an explicit `seaborg lichess upgrade` path.

Reference: https://lichess.org/api (Bot tag), spec at https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All subtasks are completed and merged
- [x] #2 `seaborg lichess` connects to a bot account, accepts challenges per policy, plays full games to completion, and submits legal moves under the game clock
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
All five subtasks (TASK-68.1 through TASK-68.5) implemented, reviewed, and merged: CLI restructured into subcommands, serde adopted workspace-wide, lichess crate scaffolded with transport and event loop, per-game loop with clock management and move submission built, and bot hardened with reconnect, rate-limit back-off, and graceful shutdown. The complete Lichess bot integration programme is done.
<!-- SECTION:FINAL_SUMMARY:END -->
