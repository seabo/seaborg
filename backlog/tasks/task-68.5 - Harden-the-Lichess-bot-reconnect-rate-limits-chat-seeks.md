---
id: TASK-68.5
title: 'Harden the Lichess bot: reconnect, rate limits, chat, seeks'
status: To Do
assignee: []
created_date: '2026-07-19 22:34'
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
