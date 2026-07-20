---
id: TASK-68.5
title: 'Harden the Lichess bot: reconnect, rate limits, chat, seeks'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 22:34'
updated_date: '2026-07-20 13:59'
labels: []
dependencies:
  - TASK-68.4
references:
  - 'https://lichess.org/api'
parent_task_id: TASK-68
priority: low
type: enhancement
ordinal: 91000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Production-hardening and optional niceties on top of a working bot (TASK-68.4). Keep each item small and independently reviewable; split further if a reviewer would prefer.

Scope:
- Stream resilience: the event stream and per-game streams drop routinely. Reconnect with exponential backoff; distinguish recoverable disconnects from terminal game-over.
- Rate limiting: honor HTTP 429 with backoff; use a single shared client/connection pool; avoid hammering endpoints.
- Graceful shutdown: on Ctrl-C stop accepting new challenges and let in-flight games finish (or resign) cleanly rather than dropping connections mid-move.
- Chat (optional): a greeting and/or 'good game' message; optionally respond to a small set of chat commands.
- Proactive play (optional): issue challenges or open seeks when idle, per config. Note this needs the `challenge:write` token scope in addition to `bot:play`.

Everything here is additive; the bot from TASK-68.4 must keep working if the optional items are deferred.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dropped event/game streams reconnect with exponential backoff without crashing or abandoning live games
- [ ] #2 HTTP 429 responses are handled with backoff and do not crash the bot
- [ ] #3 Ctrl-C triggers a graceful shutdown that stops accepting challenges and does not corrupt in-flight games
- [ ] #4 Any optional chat or proactive-challenge features added are gated by config and documented, including the challenge:write scope requirement for issuing challenges
- [ ] #5 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Scope the three mandatory hardening pillars (reconnect+backoff, HTTP 429, graceful shutdown). Defer optional chat/proactive play; AC#4 is then vacuous (no optional feature added) but document the challenge:write scope note for the deferred work.
2. New backoff.rs: pure exponential Backoff (base/max, doubling, reset); unit-tested schedule.
3. New shutdown.rs: injectable Shutdown handle (owned Arc for tests, static+signal for prod) and a Unix SIGINT/SIGTERM handler via the already-present libc; no-op on non-Unix.
4. error.rs: add RateLimited { retry_after } variant for HTTP 429.
5. transport.rs: single shared ureq::Agent (one pool); manual status handling (401 -> Unauthorized, 429 -> RateLimited with Retry-After, other non-2xx -> Http); connect/response timeouts; bounded 429 retry with backoff that aborts promptly on shutdown, factored into a testable helper.
6. client.rs: surface keepalive ticks (yield Result<Option<Event>>) so the loops can observe shutdown between real events; add resign_game.
7. run.rs: resilient event loop that reconnects with backoff, keeps a de-duplicated active-game set across reconnects (so an event-stream replay never double-spawns a worker), declines/stops accepting on shutdown, spawns workers with a Shutdown clone, and joins workers on shutdown.
8. game.rs: resilient per-game loop that reconnects on a mid-game drop but stops on a terminal status; on shutdown it resigns the in-flight game cleanly instead of making another move.
9. Tests: backoff schedule, shutdown handle, 429 retry helper, event-loop reconnect+dedup+shutdown-decline, game reconnect+shutdown-resign, all via fake transports with an injected no-op sleeper (no network, no real sleeps).
10. Run cargo fmt --check, clippy (-D warnings), cargo test --workspace.
<!-- SECTION:PLAN:END -->
