---
id: TASK-22
title: Make UCI output protocol clean and version consistent
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 17:15'
updated_date: '2026-07-18 01:13'
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
Rework for merge-eject finding (comment #3). Root cause: master advanced past base 299f5ec via TASK-12, whose test uci_new_game_is_an_owner_handled_hash_boundary asserts errors.is_empty() after 'ucinewgame\nisready\nquit'. TASK-22 routes the human startup banner to the stderr diagnostic channel, so stderr is never empty and that assertion fails when integrated.
1. Rebase this task branch onto current primary (1ae6cce) to integrate TASK-12's landed test and new_game() handling; keep TASK-22 base-to-target diff clean.
2. Reconcile the inherited test to assert diagnostics_after_banner(&errors) == "" (consistent with the other TASK-22 exact-stream tests) rather than errors.is_empty(), preserving both TASK-12's silent-ucinewgame check and TASK-22's banner-on-stderr behavior.
3. Re-verify the full integrated result: cargo build, cargo test --workspace, cargo fmt --check, cargo clippy on changed files.
4. Produce a fresh immutable target and hand off for re-review.
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

Rework (merge-eject resolution):
Resolved merge-eject finding (comment #3): the integrated result on primary failed engine::tests::uci_new_game_is_an_owner_handled_hash_boundary (asserted errors.is_empty()) because TASK-22 routes the startup banner to stderr. Rebased this task branch onto current primary (base 1ae6cce, which contains TASK-12's landed new_game() handling and that test) and changed the inherited assertion to diagnostics_after_banner(&errors) == "" — consistent with the other TASK-22 exact-stream tests. This preserves TASK-12's silent-ucinewgame guarantee (no error diagnostics) and TASK-22's banner-on-stderr behavior. No product behavior changed; only a test expectation was reconciled. New immutable target: 41b5dfd.
Verification on integrated result (base 1ae6cce): cargo build ok; cargo test --workspace ok (engine 71 passed/1 ignored incl. the reconciled test and all TASK-22 exact-stream tests, core suite, build_metadata 5, doctests ok); cargo fmt --all --check ok; cargo clippy -p engine -p seaborg --lib --bins reports no warnings citing engine/src/engine.rs or src/cmdline.rs; the pre-existing benches/square.rs E0423 exists at base 1ae6cce and is untouched by this patch (git diff 1ae6cce..target -- benches/ is empty). Manual smoke: printf 'uci\nucinewgame\nisready\nquit\n' | seaborg --uci -> stdout is pure UCI (id name seaborg 0.1.0 / id author / option / uciok / readyok); stderr = single banner 'seaborg 0.1.0 by George Seabridge (commit <12>)'; seaborg --version = seaborg 0.1.0.
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

author: @codex
created: 2026-07-18 00:58
---
Implementation handoff
Branch: task-22-uci-output-protocol
Worktree: /Users/seabo/seaborg-worktrees/task-22-uci-output-protocol
Base: 1ae6cce891de11e1439d0d3363862de2261b4aff
Implementation target: 41b5dfd134679effc0e21156aca2c100c7af4ce8
Resolved findings: merge-eject finding (comment #3, review attempt basis) — inherited uci_new_game test reconciled with TASK-22 stderr banner
Verification:
- cargo build: ok
- cargo test --workspace: ok (engine 71 passed, 1 ignored; core suite ok; build_metadata 5; doctests ok)
- cargo fmt --all -- --check: ok
- cargo clippy -p engine -p seaborg --lib --bins: no warnings cite engine/src/engine.rs or src/cmdline.rs
- manual UCI smoke: stdout pure UCI, banner isolated to stderr, --version = seaborg 0.1.0
Known failures: benches/square.rs E0423 (cargo clippy --all-targets / bench build) is a pre-existing baseline error present at base 1ae6cce; this patch does not touch benches/ (git diff 1ae6cce..41b5dfd -- benches/ is empty)
Note: rebased onto current primary 1ae6cce, so base and target SHAs supersede the prior cycle's (base 299f5ec / target 0269c0a). Prior review approval on 0269c0a is void; this new target 41b5dfd requires a fresh review.
---

author: @codex
created: 2026-07-18 01:03
---
Review attempt: 2
Reviewed branch: task-22-uci-output-protocol
Reviewed implementation: 41b5dfd134679effc0e21156aca2c100c7af4ce8
Base: 1ae6cce891de11e1439d0d3363862de2261b4aff
Verdict: approved

Scope: base-to-target diff touches only engine/src/engine.rs and src/cmdline.rs (plus this task file). Post-target commit 13a2cea is handoff metadata only (task file). Base 1ae6cce is an ancestor of the target and the target is an ancestor of the tip.

Rework verification (merge-eject finding, comment #3): the inherited TASK-12 test engine::tests::uci_new_game_is_an_owner_handled_hash_boundary changed only its assertion from errors.is_empty() to diagnostics_after_banner(&errors) == "" — this preserves TASK-12's silent-ucinewgame guarantee (no error diagnostics) while accommodating TASK-22's startup banner on stderr. Minimal and semantically sound; no product behavior changed.

Acceptance criteria (all proven):
- #1 No unsolicited non-UCI stdout before uci: startup_emits_no_stdout_and_a_trimmed_stderr_banner asserts stdout=='' for an empty script; smoke shows first stdout byte is the uci handshake 'id name'.
- #2 Errors/diagnostics never invalid protocol on stdout: malformed_and_unsupported_commands_only_write_to_stderr asserts stdout=='' while stderr carries UnexpectedToken/InvalidOption/invalid move; smoke 'register' yields empty stdout.
- #3 Single authoritative version: EngineInfo.version=env!(CARGO_PKG_VERSION) and clap #[clap(version)] both resolve to the seaborg crate 0.1.0; id name, --version, and banner all report seaborg 0.1.0 (was 0.0.2/0.1.0 drift).
- #4 Commit metadata trimmed to diagnostic channel: EngineInfo::short_commit() trims to 12 chars, written to stderr only; startup test asserts the full hash never appears; smoke stderr banner 'commit <12>'.
- #5 Exact-stream integration tests: uci_handshake_stream_is_exact and readiness_stream_is_exact assert byte-exact stdout; startup test asserts exact stdout/stderr; malformed test asserts exact empty stdout — all drive run() end-to-end.

Verification (reviewed target 41b5dfd; worktree /Users/seabo/seaborg-worktrees/task-22-uci-output-protocol):
- cargo build: ok
- cargo test --workspace: ok (engine 71 passed/1 ignored, core 35, build_metadata 5, doctests ok)
- cargo fmt --all -- --check: clean
- cargo clippy -p engine -p seaborg --lib --bins: no warning cites engine/src/engine.rs or src/cmdline.rs (pre-existing repo warnings elsewhere are unchanged from base)
- Manual: printf 'uci\nisready\nquit\n' | seaborg --uci -> stdout id name seaborg 0.1.0 / id author George Seabridge / option ... / uciok / readyok; stderr single banner; seaborg --version -> seaborg 0.1.0

Benchmarks not run: base-to-target diff touches only UCI protocol/IO plumbing (command dispatch, cmdline wiring), not movegen/search hot paths; EngineInfo is passed by reference and does not enter the search or move-generation code.

No implementation file changes after the target; approval is on 41b5dfd.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Threaded a single authoritative engine::EngineInfo (name/version=CARGO_PKG_VERSION/author/commit=GIT_HASH) from the seaborg binary into engine::run, so UCI 'id name', clap '--version', and the startup banner all derive from the seaborg package version (0.1.0), replacing three drifting sources incl. hardcoded 0.0.2. Removed the two unsolicited startup stdout writes; protocol stdout now carries only UCI traffic and the human banner (12-char trimmed commit via EngineInfo::short_commit) goes to the stderr diagnostic channel. Rework reconciled TASK-12's inherited uci_new_game test (errors.is_empty -> diagnostics_after_banner(&errors)=='') to coexist with the banner-on-stderr design. Reviewed target 41b5dfd (base 1ae6cce): cargo build ok; cargo test --workspace ok (engine 71 passed/1 ignored incl. startup/handshake/readiness/error exact-stream tests and the reconciled test, core 35, build_metadata 5, doctests); cargo fmt --all --check clean; cargo clippy on changed crates emits no warning citing engine/src/engine.rs or src/cmdline.rs; manual smoke: stdout = id name seaborg 0.1.0 / id author / option / uciok / readyok, stderr banner = 'seaborg 0.1.0 by George Seabridge (commit <12>)', --version = seaborg 0.1.0.
<!-- SECTION:FINAL_SUMMARY:END -->
