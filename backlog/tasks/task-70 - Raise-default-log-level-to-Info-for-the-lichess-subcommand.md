---
id: TASK-70
title: Raise default log level to Info for the lichess subcommand
status: To Do
assignee: []
created_date: '2026-07-20 20:06'
labels: []
dependencies: []
ordinal: 115000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The logger is initialised with `LevelFilter::Error` as its default in `src/main.rs`. The lichess crate uses `log::info!` and `log::warn!` throughout its event loop and game workers, but these messages are suppressed at the default level, so the bot runs silently and gives the operator no visibility into what it is doing. The `RUST_LOG` env var can override this today, but that is undiscoverable. The fix is to raise the default level to `Info` in `main.rs`. UCI mode does not emit any log output so a global raise is safe.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Default log level is `Info` (no `RUST_LOG` override needed)
- [ ] #2 `seaborg lichess` emits at least a startup message and challenge/game lifecycle events to stderr without any env-var configuration
- [ ] #3 UCI mode (`seaborg` with no subcommand) produces no extra output on stdout or stderr (log messages must go to stderr, which `simple_logger` already does via its `stderr` feature)
- [ ] #4 The existing `.env()` override is preserved so operators can still set `RUST_LOG` to adjust the level
<!-- AC:END -->
