---
id: TASK-68.1
title: Restructure the CLI into subcommands
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 22:33'
updated_date: '2026-07-19 22:45'
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
- [ ] #1 Bare `seaborg` with no arguments starts UCI mode
- [ ] #2 `seaborg uci`, `seaborg ui`, `seaborg perft`, `seaborg dev`, and `seaborg licenses` all dispatch to their existing behavior
- [ ] #3 The old `--uci`/`--dev`/`--ui`/`--licenses` mode flags are removed (clean break, no aliases)
- [ ] #4 `ui` subcommand still supports the port and no-open options
- [ ] #5 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
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
