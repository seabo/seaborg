---
id: TASK-68.1
title: Restructure the CLI into subcommands
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-19 22:33'
updated_date: '2026-07-19 22:59'
labels: []
dependencies: []
parent_task_id: TASK-68
priority: medium
type: task
ordinal: 87000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the current mutually-exclusive mode flags with proper subcommands. Today src/cmdline.rs uses a clap `ArgGroup("mode")` of booleans (--uci, --dev, --ui, --licenses) alongside a single `perft` subcommand, and bare `seaborg` is a no-op. This is inconsistent and can't cleanly express per-mode arguments (e.g. the upcoming `lichess` mode has a totally different arg surface than `ui`).

No backward compatibility is required — this is unreleased software. Do a clean break to subcommands; do NOT keep the old flags as aliases.

Target surface:
- `seaborg`            -> UCI on stdin/stdout (no subcommand = UCI, for chess-GUI compatibility)
- `seaborg uci`        -> explicit UCI
- `seaborg ui [--port P] [--no-open]`
- `seaborg perft ...`  (existing behavior, now a peer subcommand)
- `seaborg dev`
- `seaborg licenses`

Note: `seaborg lichess` is added by a later subtask; leave a clean place to hook it in but do not implement it here. Keep the existing dispatch targets (engine::launch, run_ui, dev, perft, licenses) intact — this is a routing refactor, not a behavior change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Bare `seaborg` with no arguments starts UCI mode
- [x] #2 `seaborg uci`, `seaborg ui`, `seaborg perft`, `seaborg dev`, and `seaborg licenses` all dispatch to their existing behavior
- [x] #3 The old `--uci`/`--dev`/`--ui`/`--licenses` mode flags are removed (clean break, no aliases)
- [x] #4 `ui` subcommand still supports the port and no-open options
- [x] #5 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace the ArgGroup("mode") booleans in src/cmdline.rs with a required-less Option<Commands> subcommand enum.
2. Add variants: Uci, Ui(UiArgs{port, no_open}), Perft(PerftArgs), Dev, Licenses. Keep Perft as-is.
3. Bare seaborg (command == None) dispatches to UCI, same as explicit Uci.
4. Rewrite cmdline() to match on the subcommand and call the existing dispatch targets (engine::launch, run_ui, dev, perft, licenses).
5. Rework run_ui to take a UiArgs instead of &Args.
6. Leave a clear place to hook in the future lichess subcommand without implementing it.
7. Run fmt, clippy, and workspace tests.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Restructured src/cmdline.rs from ArgGroup mode booleans to a clap subcommand enum (Uci/Ui/Perft/Dev/Licenses). Bare seaborg dispatches to UCI via unwrap_or(Commands::Uci). UI arguments moved into a dedicated UiArgs struct exposing --port and --no-open; run_ui now takes &UiArgs. Old --uci/--dev/--ui/--licenses flags removed with no aliases. Left a comment marker where the future lichess subcommand hooks in. Verified dispatch via smoke tests: bare/explicit uci both emit UCI id lines, perft -n 2 = 400 nodes, licenses prints the artwork notice, ui --help shows --port/--no-open, and --uci now errors as an unexpected argument.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-19 22:49
---
Implementation handoff
Branch: task-68.1-cli-subcommands
Worktree: /Users/seabo/seaborg-worktrees/task-68.1-cli-subcommands
Base: 064f883e63cb04883cc3c764d15dd520f7e59441
Implementation target: 3d1e99a
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (all suites 0 failed)
Known failures: none
---

author: @george
created: 2026-07-19 22:59
---
Review attempt: 1
Reviewed branch: task-68.1-cli-subcommands
Reviewed implementation: 3d1e99a
Verdict: approved

All acceptance criteria proven by direct execution of the built binary and by inspecting the full base(064f883)->target(3d1e99a) diff. The change is a clean routing refactor confined to src/cmdline.rs: mode ArgGroup booleans replaced by a subcommand enum, per-mode args isolated in UiArgs, dispatch targets unchanged, no #[allow] added, no old-flag aliases retained. Comments are self-contained. No hot-path (movegen/search) code touched, so speed benchmarks are not applicable.

Verification (on the implementation target code; only the backlog task file differs between 3d1e99a and this approval commit):
- bare seaborg (stdin uci/quit): emits 'id name seaborg', 'uciok' -> UCI (AC1)
- seaborg uci / ui --help / perft -n 2 / dev / licenses: all dispatch to existing behavior (AC2, AC4)
- seaborg --uci|--dev|--ui|--licenses: 'error: unexpected argument found' (AC3)
- ui --help lists --port <PORT> and --no-open (AC4)
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings (CARGO_TARGET_DIR=/tmp clean): pass, no warnings
- cargo test --workspace: pass (279+45+19+1+doctests, 0 failed) (AC5)
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Restructured src/cmdline.rs from clap ArgGroup mode booleans to an Option<Commands> subcommand enum (Uci/Ui/Perft/Dev/Licenses), with UI args in a dedicated UiArgs struct exposing --port/--no-open. Bare seaborg dispatches to UCI via unwrap_or(Commands::Uci). Existing dispatch targets (engine::launch, run_ui, perft, dev, licenses) are unchanged; a comment marks where the future lichess subcommand hooks in. Verified on target 3d1e99a: bare seaborg and 'uci' both emit UCI id/uciok lines, 'ui --help' shows --port/--no-open, 'perft -n 2' runs, 'dev' runs the threefold demo, 'licenses' prints the artwork notice, and all removed flags (--uci/--dev/--ui/--licenses) now error as unexpected arguments. cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR), and cargo test --workspace (344 tests, 0 failed) all pass.
<!-- SECTION:FINAL_SUMMARY:END -->
