---
id: TASK-22
title: Make UCI output protocol clean and version consistent
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:15'
updated_date: '2026-07-17 23:36'
labels:
  - uci
  - release
dependencies:
  - TASK-1.1
references:
  - engine/src/engine.rs
  - src/main.rs
  - Cargo.toml
priority: medium
type: bug
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The process emits unsolicited startup text and several diagnostics on protocol stdout, while Cargo metadata and the engine banner report different versions. Ensure GUI-facing stdout contains only valid UCI traffic and derive one consistent engine identity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Starting UCI mode emits no unsolicited non-UCI stdout before the uci command
- [ ] #2 Errors and optional human diagnostics do not appear as invalid protocol messages on stdout
- [ ] #3 The id name response, command-line version, and startup metadata derive from one authoritative package version
- [ ] #4 Commit metadata is trimmed and, when shown, is emitted through an appropriate diagnostic channel or UCI info form
- [ ] #5 Integration tests assert the exact startup, uci handshake, error, and readiness output streams
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Thread a single authoritative engine identity (name, version=CARGO_PKG_VERSION, author, commit=GIT_HASH) from the seaborg binary into engine::launch, replacing hardcoded '0.0.2' strings so id name, --version, and startup metadata share one source.
2. Remove the unsolicited startup banner + 'commit:' line from protocol stdout; emit a single human diagnostic banner (with trimmed short commit) to stderr instead so no non-UCI stdout precedes the uci command.
3. Update the 'uci' handshake to emit 'id name <name> <version>' derived from the threaded identity.
4. Ensure errors/diagnostics never appear as invalid protocol messages on stdout (verify existing stderr routing; keep commit metadata on diagnostic channel).
5. Add/strengthen integration tests asserting exact startup, uci handshake, error, and readiness stdout streams; update existing tests referencing the old banner.
6. Run cargo build, cargo test, cargo fmt --check, cargo clippy.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation summary:
- Added engine::EngineInfo { name, version, author, commit } threaded from the seaborg binary (src/cmdline.rs) into engine::launch/run/handle_command. The binary supplies version=env!(\"CARGO_PKG_VERSION\") and commit=env!(\"GIT_HASH\"), so UCI `id name`, clap `--version`, and the startup banner all derive from the single seaborg package version (previously three inconsistent sources: Cargo 0.1.0, banner/id-name hardcoded 0.0.2).
- Removed the two unsolicited stdout writes at startup (banner + `commit:` line). Protocol stdout now contains only valid UCI traffic; the first stdout byte comes from the `uci` handshake.
- The human banner, including a trimmed 12-char short commit, is written once to the stderr diagnostic channel via EngineInfo::short_commit().
- `uci` handshake now emits `id name <name> <version>` / `id author <author>` from the threaded identity.
- Error/parse-failure routing to stderr was already in place and is preserved; tests now assert stdout stays empty for malformed/unsupported input.
- Tests (engine::tests, driving run() directly): added startup_emits_no_stdout_and_a_trimmed_stderr_banner, uci_handshake_stream_is_exact, readiness_stream_is_exact; updated eof/replacement/standard-state/malformed tests to assert exact stdout streams and banner-only stderr.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 23:36
---
Implementation handoff
Branch: task-22-uci-output-protocol
Worktree: /Users/seabo/seaborg-worktrees/task-22-uci-output-protocol
Base: 299f5ec549b6b6983ff0b0f08b4460bfb1853335
Implementation target: 0269c0a51ba19a646f3e86a0a5657aba66fcd1de
Resolved findings: none (initial implementation)
Verification:
- cargo build: ok
- cargo test --workspace: ok (core 35 passed; engine 63 passed, 1 pre-existing ignored; seaborg + build_metadata integration ok)
- cargo fmt --all -- --check: ok
- cargo clippy --workspace --all-targets: no new warnings in engine/src/engine.rs or src/cmdline.rs (repo has pre-existing warnings elsewhere)
- Manual UCI smoke (printf 'uci\nisready\nquit\n' | seaborg --uci): stdout = id name seaborg 0.1.0 / id author George Seabridge / option ... / uciok / readyok; stderr banner = "seaborg 0.1.0 by George Seabridge (commit 286ae44e1559)"; seaborg --version = "seaborg 0.1.0"
Known failures: none
---
<!-- COMMENTS:END -->
