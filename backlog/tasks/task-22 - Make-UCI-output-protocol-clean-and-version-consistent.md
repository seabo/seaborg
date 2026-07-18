---
id: TASK-22
title: Make UCI output protocol clean and version consistent
status: Changes Requested
assignee:
  - '@codex'
created_date: '2026-07-17 17:15'
updated_date: '2026-07-18 00:48'
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
- [x] #1 Starting UCI mode emits no unsolicited non-UCI stdout before the uci command
- [x] #2 Errors and optional human diagnostics do not appear as invalid protocol messages on stdout
- [x] #3 The id name response, command-line version, and startup metadata derive from one authoritative package version
- [x] #4 Commit metadata is trimmed and, when shown, is emitted through an appropriate diagnostic channel or UCI info form
- [x] #5 Integration tests assert the exact startup, uci handshake, error, and readiness output streams
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

author: @codex
created: 2026-07-18 00:43
---
Review attempt: 1
Reviewed branch: task-22-uci-output-protocol
Reviewed implementation: 0269c0a51ba19a646f3e86a0a5657aba66fcd1de
Base: 299f5ec549b6b6983ff0b0f08b4460bfb1853335
Verdict: approved

Scope: base-to-target diff touches only engine/src/engine.rs and src/cmdline.rs (plus lifecycle metadata). No unrelated changes; single post-target commit f8fdc78 is handoff metadata only.

Acceptance criteria (all proven):
- #1 No unsolicited non-UCI stdout before uci: empty-script run yields stdout=='' (test startup_emits_no_stdout_and_a_trimmed_stderr_banner); smoke stdout begins with id name in response to uci.
- #2 Errors/diagnostics never invalid protocol on stdout: malformed_and_unsupported_commands_only_write_to_stderr asserts stdout=='' while stderr carries the errors.
- #3 Single authoritative version: id name, clap --version, and banner all derive from the seaborg crate CARGO_PKG_VERSION; all report seaborg 0.1.0 (was 0.0.2/0.1.0 drift).
- #4 Commit metadata trimmed to a diagnostic channel: EngineInfo::short_commit() trims to 12 chars and is written to stderr only; test asserts the full hash never appears.
- #5 Exact-stream integration tests: uci_handshake_stream_is_exact, readiness_stream_is_exact, startup banner test, and the malformed-error test assert exact startup/handshake/error/readiness streams by driving run() end-to-end.

Verification (target code, worktree /Users/seabo/seaborg-worktrees/task-22-uci-output-protocol):
- cargo build: ok
- cargo test --workspace: ok (engine 63 passed/1 ignored, core 35, build_metadata 5, doctests ok)
- cargo fmt --all -- --check: ok
- cargo clippy: no warnings cite engine/src/engine.rs or src/cmdline.rs; pre-existing repo warnings and the benches/square.rs E0423 exist identically at base and are untouched by this patch (not patch-introduced)
- Manual: printf 'uci\nisready\nquit\n' | seaborg --uci -> stdout id name seaborg 0.1.0 / id author George Seabridge / option ... / uciok / readyok; stderr banner seaborg 0.1.0 by George Seabridge (commit ...); seaborg --version -> seaborg 0.1.0

Benchmarks not run: diff touches only UCI protocol/IO plumbing, not movegen/search hot paths.
---

author: @codex
created: 2026-07-18 00:48
---
Merge attempt: 1
Verdict: ejected -> Changes Requested
Primary tip tested (T): 52e97622f3dbe75f773313d7710f4a6b22077b0a
Approved target: 0269c0a51ba19a646f3e86a0a5657aba66fcd1de
Trial merge (discarded): non-ff merge of approval tip 407f1dc into T

Result: textually clean merge, but the integrated result FAILS a required check. Not landed; primary left at T.

Failing command: cargo test --workspace (run on the trial merge commit)
Failure:
  engine::tests::uci_new_game_is_an_owner_handled_hash_boundary ... FAILED
  panicked at engine/src/engine.rs:393: assertion failed: errors.is_empty()
  (engine suite: 67 passed; 1 failed; 1 ignored)

Root cause (semantic conflict, not textual): master advanced past this task's base 299f5ec via TASK-12 (2b1fe16 'fix(task-12): preserve tt reuse and mate distance'), which added the test uci_new_game_is_an_owner_handled_hash_boundary asserting errors.is_empty() after a 'ucinewgame\nisready\nquit' script. TASK-22 intentionally moves the startup human banner onto the stderr diagnostic channel, so run_script now always leaves the banner on stderr and errors.is_empty() is false. The two changes are individually correct but collide on the shared stderr expectation.

Required rework (on this task branch, reusing the worktree): rebase/rework onto current primary and reconcile the stderr-diagnostics expectation with TASK-12's now-landed test. Options for the implementer to choose: update the inherited uci_new_game test to assert diagnostics_after_banner(&errors) == "" (consistent with the other TASK-22 tests) rather than errors.is_empty(); this keeps both behaviors. Re-verify the full suite on a fresh integrated result. No code was changed by this merge attempt; approval on 0269c0a is void until a new target is produced and re-reviewed.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Threaded a single authoritative engine::EngineInfo (name/version=CARGO_PKG_VERSION/author/commit=GIT_HASH) from the seaborg binary into engine::run, so UCI 'id name', clap '--version', and the startup banner all report seaborg 0.1.0 (previously three drifting sources incl. hardcoded 0.0.2). Removed the two unsolicited stdout writes; protocol stdout now carries only UCI traffic and the human banner (12-char trimmed commit) goes to stderr. Verified target 0269c0a: cargo build ok; cargo test --workspace ok (engine 63 passed incl. new startup/handshake/readiness/error exact-stream tests, core 35, build_metadata 5); cargo fmt --all --check ok; cargo clippy on changed files clean (pre-existing repo warnings and the benches/square.rs E0423 are baseline, untouched by this patch); manual smoke: stdout = id name seaborg 0.1.0 / id author / option / uciok / readyok with stderr banner 'seaborg 0.1.0 by George Seabridge (commit ...)' and --version = seaborg 0.1.0.
<!-- SECTION:FINAL_SUMMARY:END -->
