---
id: TASK-70
title: Raise default log level to Info for the lichess subcommand
status: Done
assignee:
  - '@george'
created_date: '2026-07-20 20:06'
updated_date: '2026-07-20 20:31'
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
- [x] #1 Default log level is `Info` (no `RUST_LOG` override needed)
- [x] #2 `seaborg lichess` emits at least a startup message and challenge/game lifecycle events to stderr without any env-var configuration
- [x] #3 UCI mode (`seaborg` with no subcommand) produces no extra output on stdout or stderr (log messages must go to stderr, which `simple_logger` already does via its `stderr` feature)
- [x] #4 The existing `.env()` override is preserved so operators can still set `RUST_LOG` to adjust the level
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

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-20 20:13
---
Implementation handoff
Branch: task-70-lichess-default-log-info
Worktree: /Users/seabo/seaborg-worktrees/task-70-lichess-default-log-info
Base: 02e3ba54808be16a5b6d4d0cf80e54459df29867
Implementation target: 43d54e38100c99bd19e751ff0419262e134bf5c6
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (all suites, 0 failed)
- Manual UCI silence check (printf 'uci\nquit' | seaborg): stdout clean UCI handshake; stderr has only the pre-existing engine banner (engine.rs:115), no log lines
Known failures: none

Note for reviewer: the single stderr line in UCI mode is the human startup banner intentionally written to stderr in engine.rs:115; it exists on master and is not a log message, so it does not conflict with AC#3 (which concerns log output). No log output is emitted in UCI mode after this change.
---

author: @george
created: 2026-07-20 20:18
---
Review attempt: 1
Reviewed branch: task-70-lichess-default-log-info
Reviewed implementation: 43d54e38100c99bd19e751ff0419262e134bf5c6
Verdict: approved

Base-to-target diff is scope-disciplined: only src/main.rs (3 lines) plus task metadata; no #[allow] added; comments explain the rationale without referencing task IDs or restating code.

Acceptance criteria evidence:
- AC#1: src/main.rs sets .with_level(LevelFilter::Info); logger confirmed active on stderr and honoring RUST_LOG at runtime.
- AC#2: Info default + lichess log::info!/warn! at connection (lichess/src/run.rs:52) and game lifecycle (lichess/src/game.rs, lichess/src/run.rs) emit at the default level. Live end-to-end emission requires a real bot token and network (environmental, not a code property); proven deductively from the Info threshold plus the info!/warn! call sites.
- AC#3: printf 'uci\nquit' | seaborg (no RUST_LOG): stdout is a clean UCI handshake; stderr contains only the pre-existing engine.rs:115 human banner (a writeln! present on base 02e3ba5, not a log message); the downgraded debug! 'logger initialized' line is suppressed (0 occurrences). No new UCI output.
- AC#4: .env() preserved; RUST_LOG=debug renders 'DEBUG [seaborg] logger initialized' to stderr, confirming the override still adjusts the level.

Verification commands (on 43d54e3):
- cargo fmt --check: pass
- CARGO_TARGET_DIR=/tmp/task70-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (chess 45, engine 300, lichess 68, integration suites; 0 failed)
- Manual UCI silence check + RUST_LOG override check: as described above

Not a movegen/search hot path, so benchmarks were not required. No implementation file changed between the reviewed target and this approval commit.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Raised the SimpleLogger default from Error to Info in src/main.rs so the Lichess bot's lifecycle logs are visible without RUST_LOG, and downgraded the startup 'logger initialized' line to debug! so UCI mode stays silent. Verified on implementation target 43d54e38100c99bd19e751ff0419262e134bf5c6: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings pass (clean CARGO_TARGET_DIR); cargo test --workspace pass (all suites, 0 failed). Runtime: UCI ('uci\nquit') stdout is a clean handshake and stderr carries only the pre-existing engine.rs:115 banner with no log lines; RUST_LOG=debug renders the debug line to stderr confirming .env() override and stderr wiring.
<!-- SECTION:FINAL_SUMMARY:END -->
