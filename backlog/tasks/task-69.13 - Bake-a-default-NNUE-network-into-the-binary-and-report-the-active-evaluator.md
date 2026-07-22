---
id: TASK-69.13
title: Bake a default NNUE network into the binary and report the active evaluator
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-22 12:05'
updated_date: '2026-07-22 12:41'
labels:
  - nnue
  - uci
  - build
  - dx
dependencies: []
parent_task_id: TASK-69
priority: high
ordinal: 131000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Today a Seaborg binary plays with the hand-crafted evaluation unless the operator manually issues `setoption name EvalFile value /abs/path/gen-NNN.sbnn` inside a live UCI session. That makes real playing strength invisible by default: a plain `cargo build --release` produces a binary weaker than the best evaluator the project has, every strength test and Lichess deployment depends on out-of-band setup that is easy to forget, and nothing the engine prints states which evaluator a given process is actually running.

Goal. A network is embedded in the binary at compile time and used as the default evaluator, and the engine states on startup exactly which evaluator it is running.

Design intent (decided with the requester; a plan may refine mechanism but not these outcomes):

- The default network is committed in-repo (for example `engine/nets/default.sbnn`) and embedded with `include_bytes!`, so a fresh clone builds a full-strength binary with no operator setup. Only promoted defaults are committed, not every training generation.
- Embedding sits behind a default-on Cargo feature, so `--no-default-features` still builds a hand-crafted-eval binary and the crate stays buildable and testable with the feature off.
- The embedded bytes go through the same `Network::read` validation path as an `EvalFile` load: no second, laxer parser.
- Evaluator identity is reported to the operator: which network (a stable identifier plus the parameter hash and hidden width from the header), or explicitly that the build runs the hand-crafted evaluation. The report must appear at startup and again whenever the active evaluator changes, and must not corrupt the UCI stream (stdout stays protocol-clean; use the diagnostic banner and/or `info string` at protocol-legal points).
- EvalFile precedence and semantics: an explicit path still overrides the embedded default; the UCI convention value `<empty>` returns to the built-in default (the embedded network, or the hand-crafted evaluation in a build without one); a new explicit value `none` selects the hand-crafted evaluation. This changes what `<empty>` means today, where it selects the hand-crafted evaluation, so the advertised option default and the docs must move with it.
- Non-UCI entry points (the `--network` flag of the `datagen` subcommand, the lichess client, self-play) must have their behaviour stated deliberately rather than changed by accident. The bootstrap programme depends on datagen being able to run against the hand-crafted evaluation, so datagen must not silently start using a baked network.

Why now. TASK-69.11 and TASK-69.12 produce promoted networks that the shipped binary must then carry; without this, every consumer of a promoted net repeats the same manual wiring, and benchmark attribution (see `docs/strength-testing.md` and BENCHMARKS.md) has no reliable record of which evaluator a measured binary used.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A release build with default features and no runtime options evaluates with the embedded network; this is demonstrated by a test or a documented reproducible check rather than asserted
- [ ] #2 Building with `--no-default-features` produces a working hand-crafted-eval binary, and the workspace builds, clippy-passes, and tests cleanly both with the embedding feature on and off
- [ ] #3 The embedded network is validated by the same loader used for EvalFile, and a test parses the embedded bytes and asserts the resulting architecture
- [ ] #4 At startup the engine reports the active evaluator, naming the embedded network with its parameter hash and hidden width, or stating explicitly that the build uses the hand-crafted evaluation
- [ ] #5 The evaluator report is re-emitted whenever the active evaluator changes via `setoption name EvalFile`, and stdout remains valid UCI at all times (verified by a driver-level test over the command stream)
- [ ] #6 An explicit `setoption name EvalFile value <path>` overrides the embedded default; `value <empty>` restores the built-in default; `value none` selects the hand-crafted evaluation; each case is covered by a test
- [ ] #7 The `uci` option advertisement states a default consistent with the actual built-in default
- [ ] #8 Behaviour of `datagen --network`, the lichess client, and self-play under an embedded-net build is deliberate, documented, and covered by a test where it differs from the UCI driver; the hand-crafted-eval datagen path used by the bootstrap programme remains reachable
- [ ] #9 Documentation records how to promote and re-bake a default network, how to build without one, and how to read the evaluator identity of a binary from its output
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Commit the promoted network as engine/nets/default.sbnn (gen-000: H=256, qa=255, qb=64, scale=400, param hash 0xdaf86bb3d50cec6b) and record its provenance identifier in source.
2. Add a default-on 'embedded-net' Cargo feature to engine; set default-features=false on the engine dependency in lichess and the root seaborg package and re-export the feature from each, so --no-default-features at the root actually reaches engine rather than being re-enabled by feature unification.
3. New engine module (nnue::embedded) exposing the include_bytes! blob, its stable identifier, and a OnceLock-cached Option<Arc<Network>> parsed through Network::read — no second parser. Test: the embedded bytes parse and the architecture matches.
4. Add Network::param_hash() so the header identity a file declares can be reported from a loaded network.
5. Introduce an Evaluator identity value (hand-crafted / built-in net / file net, each with hidden width and parameter hash) with a Display used for the report.
6. SearchEngine::new starts from the built-in default network. Self-play already calls set_network explicitly from SelfPlayConfig, so datagen stays hand-crafted unless --network is given; pin that with a test. Lichess picks up the built-in default by construction.
7. UCI: EngineOpt::EvalFile carries a three-way setting (Default / HandCrafted / Path). <empty> restores the built-in default, 'none' selects the hand-crafted evaluation, a path overrides. Parser + driver tests for all three.
8. Report the active evaluator on the diagnostic channel at startup and on every successful evaluator change, leaving stdout protocol-clean; driver-level test over the command stream asserts both the report and stdout validity.
9. Update tools/rl gen-0 bootstrap to pass EvalFile=none for the hand-crafted baseline, which no longer follows from omitting the option.
10. Document promotion/re-baking, building without a net, and reading evaluator identity; run fmt, clippy, and tests with the feature on and off.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation
- Committed the promoted gen-000 network as engine/nets/default.sbnn (768x256, qa 255, qb 64, scale 400, parameter hash 0xdaf86bb3d50cec6b) and linked it in with include_bytes! from the new engine/src/nnue/embedded.rs, behind the default-on 'embedded-net' feature. The bytes are parsed by Network::read — the same loader an EvalFile path takes — once per process and shared behind an Arc.
- Feature plumbing: lichess and the root seaborg package depend on engine with default-features = false and re-export their own 'embedded-net', because Cargo unifies features across the graph and a single edge left on the engine's defaults would silently undo --no-default-features for the whole build.
- SearchEngine::new now starts on the built-in network, so full strength is the default rather than something each consumer opts into. Self-play sets its evaluator from SelfPlayConfig unconditionally, so datagen remains hand-crafted unless --network names a file; the Lichess bot picks the built-in network up and logs it at connect.
- EvalFile is now three-valued (EvalFileSetting::BuiltInDefault / HandCrafted / File): <empty> restores the build's built-in evaluator, 'none' selects the hand-crafted evaluation, a path overrides. The advertised 'default <empty>' is truthful in both builds because <empty> is by definition the state a session starts in.
- New nnue::ActiveEvaluator names the live evaluator (built-in id or file path, hidden width, parameter hash from Network::param_hash()). The UCI driver reports it at startup and after every evaluator change that took effect, on the diagnostic channel — stdout stays protocol-clean, and stderr is legal before the uci handshake where 'info string' would not be.
- tools/rl now passes --baseline-option EvalFile=none for the generation-0 bootstrap; omitting the option would have gated candidates against the baked-in network while the ledger recorded the baseline as 'handcrafted'. Pinned by a new GateCommandTests case against the real SubprocessBackend command builder.
- Docs: new docs/default-network.md (identity reporting, EvalFile semantics, building without a net, non-UCI entry points, the promote/re-bake procedure); README build + UCI-option sections; docs/strength-testing.md and tools/rl/README.md corrected for the changed default.

Two pre-existing tests assumed a fresh SearchEngine evaluates hand-crafted and now ask for it explicitly: search_engine_starts_searches_with_the_configured_network (which also asserts the fresh engine carries the built-in network) and child_mate_windows_preserve_distance_parity (whose expected iteration depends on the leaf values).

Real-binary smoke on the release build: startup prints 'evaluator: NNUE built-in gen-000 (hidden width 256, parameter hash 0xdaf86bb3d50cec6b)'; setoption EvalFile none then <empty> re-report 'hand-crafted evaluation' and the built-in in turn, with stdout carrying only uci/info/bestmove; --no-default-features prints 'evaluator: hand-crafted evaluation'; datagen on the embedded build still reports 'evaluator: hand-crafted'.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-22 12:41
---
Implementation handoff
Branch: task-69.13-bake-default-network
Worktree: /Users/seabo/seaborg-worktrees/task-69.13-bake-default-network
Base: 30e530a14690aff8ec4e1a46508d8c4d990b28cd
Implementation target: aa3cefc
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo clippy --workspace --all-targets --no-default-features -- -D warnings: clean
- cargo test --workspace: pass (614 passed, 0 failed)
- cargo test --workspace --no-default-features: pass (613 passed, 0 failed; the embedded-net-only test does not compile in that build)
- python3 -m unittest in tools/rl: 15 pass; in tools/strength: 21 pass
- release-binary smoke (cargo build --release): startup reports 'evaluator: NNUE built-in gen-000 (hidden width 256, parameter hash 0xdaf86bb3d50cec6b)'; EvalFile none then <empty> re-report hand-crafted and the built-in in turn with stdout carrying only uci/info/bestmove; --no-default-features release binary reports 'evaluator: hand-crafted evaluation'; datagen on the embedded build reports 'evaluator: hand-crafted'
Known failures: none
---
<!-- COMMENTS:END -->
