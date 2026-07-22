---
id: TASK-69.13
title: Bake a default NNUE network into the binary and report the active evaluator
status: To Do
assignee: []
created_date: '2026-07-22 12:05'
updated_date: '2026-07-22 12:05'
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
