---
id: TASK-68.3
title: 'Scaffold the lichess crate: transport, config, event loop'
status: In Review
assignee:
  - '@george'
created_date: '2026-07-19 22:33'
updated_date: '2026-07-19 23:29'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add lichess crate to workspace members; lichess/Cargo.toml depends on core+engine (path) plus ureq=3 (rustls) and toml=1; serde/serde_json inherited from workspace. Add lichess path dep to root seaborg binary.
2. lichess modules: error (Error enum + Result), config (Config/ChallengePolicy/EngineSettings with serde defaults + Config::load with default path and --config override), transport (Transport trait + ureq HttpTransport with bearer token + NDJSON streaming), event (serde tagged Event/Challenge/TimeControl types + NDJSON line parsing tolerating keepalive blanks), policy (evaluate challenge vs policy + games cap -> Accept/Decline{reason}), account (Account + is_bot + game count), client (LichessClient<T: Transport> typed methods: account/accept/decline/upgrade/event_stream), game (documented future game-runner handoff genuinely using engine options + core Position), run (load_token from LICHESS_BOT_TOKEN, run(), upgrade() with confirmation closure, event loop respecting max-games cap and non-bot detection).
3. Wire seaborg lichess and seaborg lichess upgrade into cmdline dispatch (clap subcommand with --config and upgrade subcommand; upgrade prompts stdin confirmation).
4. Unit tests: event dispatch vs inline NDJSON fixtures via a FakeTransport recording accept/decline (no network), policy matrix, config defaults+parse, account bot detection, engine_options mapping.
5. Run cargo fmt --check, clippy -D warnings, cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the lichess crate scaffold.

Layout (lichess/src): error (Error/Result), config (Config/ChallengePolicy/EngineSettings with serde(default, deny_unknown_fields) + validation + Config::load with default path seaborg-lichess.toml and --config override), transport (Transport trait + ureq HttpTransport, bearer token, NDJSON line streaming), event (serde internally-tagged Event/TimeControl using rename_all + rename_all_fields; parse_line tolerates blank keepalive lines; #[serde(other)] tolerates unknown event types), policy (evaluate challenge vs policy + games cap -> Accept/Decline{reason} with Lichess reason strings), account (Account is_bot + games_played), client (LichessClient<T: Transport> typed methods account/accept/decline/upgrade/event_stream), game (GameHandoff seam using engine::options::EngineOpt + core Position::start_pos), run (load_token, run->serve, upgrade with confirmation closure, run_event_loop respecting max-games cap and non-bot detection).

Design decisions:
- Transport abstraction is the network seam. Tests use a FakeTransport that replays inline NDJSON and records POSTs, so challenge handling runs with no network (AC#6).
- Upgrade takes a confirmation closure; the CLI supplies the stdin 'yes' prompt (src/cmdline.rs), keeping lib free of terminal I/O and unit-testable. Upgrade requires zero games (Error::UpgradeIneligible) and reports AlreadyBot/Cancelled/Upgraded.
- Non-bot accounts are rejected in serve() before streaming, with an error message pointing at 'seaborg lichess upgrade' (AC#5).
- ureq 3 default TLS backend is rustls (rustls/rustls-webpki pulled into Cargo.lock). ureq and toml are single-consumer, so they live in lichess/Cargo.toml; serde/serde_json are inherited from the workspace per manifest policy.
- Added lichess/seaborg-lichess.example.toml documenting every field and its default (AC#3).

Deferred (out of scope): playing moves (TASK-68.4) and reconnect/backoff (TASK-68.5). GameStart currently logs and builds the GameHandoff without spawning a runner.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-19 23:29
---
Implementation handoff
Branch: task-68.3-scaffold-lichess-crate
Worktree: /Users/seabo/seaborg-worktrees/task-68.3-scaffold-lichess-crate
Base: f8cc36b621173b93ea93d78f9e43c0ec66d68767
Implementation target: 8f028cdc704f705b0cfb25fa873db6136f83d229
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (lichess 33; engine 269 + 45 + 19 + 1; 0 failed; 2 pre-existing engine tests ignored)
Known failures: none
---
<!-- COMMENTS:END -->
