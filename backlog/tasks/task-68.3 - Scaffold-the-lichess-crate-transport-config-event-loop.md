---
id: TASK-68.3
title: 'Scaffold the lichess crate: transport, config, event loop'
status: To Do
assignee: []
created_date: '2026-07-19 22:33'
labels: []
dependencies:
  - TASK-68.1
  - TASK-68.2
references:
  - 'https://lichess.org/api'
parent_task_id: TASK-68
priority: medium
type: feature
ordinal: 89000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create the new `lichess` workspace crate and everything needed to connect and manage challenges — but not yet play moves (that is the next subtask).

Scope:
- New `lichess` crate added to workspace members, depending on `engine` and `core`. Add `ureq` (rustls TLS) and `serde`/`serde_json` (already workspace deps after TASK-68.2). Follow the workspace manifest policy.
- HTTP transport: a small synchronous client over `ureq`. Put the transport behind a trait so the event/game logic can be unit-tested against recorded NDJSON fixtures with no network. Shared client with the Authorization bearer token.
- Auth/config loading: read the token from the `LICHESS_BOT_TOKEN` env var; load a TOML config file (default path plus `--config PATH`) covering the challenge-acceptance policy (allowed variants — default standard only; rated/casual; min/max initial time and increment; min/max opponent rating; accept from bots and/or humans), max concurrent games, and engine settings (hash MB, move-time safety margin). Provide sensible defaults.
- Event stream: open GET /api/stream/event, read NDJSON line-by-line (tolerating keepalive blank lines), and dispatch `challenge` -> apply acceptance policy -> POST /api/challenge/{id}/accept or /decline; `gameStart` -> log and hand off to the (future) game runner respecting the max-games cap.
- Account upgrade path: `seaborg lichess upgrade` performs POST /api/bot/account/upgrade behind an explicit confirmation, and normal startup detects a non-bot account and exits with a message pointing at upgrade. Upgrade is irreversible and requires a zero-game account and the `bot:play` scope.
- Wire `seaborg lichess` (and `seaborg lichess upgrade`) into the subcommand dispatch from TASK-68.1.

Out of scope: playing moves in a game (TASK-68.4) and reconnect/backoff hardening (TASK-68.5).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `lichess` crate exists in the workspace, builds, and depends on engine + core with ureq and serde
- [ ] #2 Token is read from LICHESS_BOT_TOKEN; a missing/invalid token fails fast with a clear message
- [ ] #3 A TOML config controls the challenge-acceptance policy and engine settings, with documented defaults when the file is absent
- [ ] #4 `seaborg lichess` opens the event stream and accepts or declines incoming challenges according to the configured policy
- [ ] #5 `seaborg lichess upgrade` upgrades the account behind an explicit confirmation, and a non-bot account is detected on normal startup with a message pointing at upgrade
- [ ] #6 The HTTP transport is abstracted so event handling is unit-tested against NDJSON fixtures without network access
- [ ] #7 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->
