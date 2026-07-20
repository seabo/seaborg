---
id: TASK-70
title: Raise default log level to Info for the lichess subcommand
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-20 20:06'
updated_date: '2026-07-20 20:13'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Change SimpleLogger default level in src/main.rs from LevelFilter::Error to LevelFilter::Info; preserve .env() override.
2. Verify UCI mode emits no stdout/stderr output (log goes to stderr via simple_logger stderr feature; only 'logger initialized' info line is emitted at startup which is acceptable stderr diagnostic, but confirm UCI protocol on stdout is unaffected).
3. Run required checks: cargo fmt --check, clippy, cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Raised SimpleLogger default from LevelFilter::Error to LevelFilter::Info in src/main.rs, preserving the .env() override so RUST_LOG still adjusts the level. Downgraded the startup 'logger initialized' line from info! to debug! so it stays suppressed at the default level and UCI mode emits no log output. Verified: engine UCI path contains no log macro calls (only startup log was the main.rs line). Manual UCI check ('uci\nquit'): stdout is clean UCI traffic; stderr contains only the pre-existing human banner from engine.rs:115 (a deliberate writeln! to stderr, not a log message, present on master). lichess crate emits log::info!/warn! for connection ('connected to Lichess as bot ...'), challenges, and game lifecycle events, now visible at the Info default.
<!-- SECTION:NOTES:END -->
