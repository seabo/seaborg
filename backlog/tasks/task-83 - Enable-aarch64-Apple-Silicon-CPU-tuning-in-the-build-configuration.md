---
id: TASK-83
title: Enable aarch64 (Apple Silicon) CPU tuning in the build configuration
status: To Do
assignee: []
created_date: '2026-07-25 12:21'
labels: []
dependencies: []
references:
  - .cargo/config.toml
  - engine/src/nnue/accumulator.rs
  - engine/src/nnue/inference.rs
ordinal: 139000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The build config gates rustflags per target: x86_64 gets `target-cpu=x86-64-v2` (SSE4.2 + POPCNT) at `.cargo/config.toml:23-24`, but the aarch64/fallback arm at `.cargo/config.toml:26-31` sets no rustflags, so Apple Silicon (M3) builds compile only against the default armv8-a baseline. Modern ARMv8.2+ extensions the M3 supports — notably FEAT_DotProd (`sdot`/`udot`) and FEAT_I8MM (`smmla`) — are never enabled, so the autovectorizer cannot use them on the hot NNUE loops (the scalar accumulator update at `engine/src/nnue/accumulator.rs:163-189` and the scalar inference fallback at `engine/src/nnue/inference.rs:142-148`). Choose and apply an aarch64 target-cpu/target-feature policy that lets the compiler emit modern M3 instructions where safe, while preserving portability and the distributable-build guarantees established for x86 (do not silently pin a baseline that breaks generic/older aarch64 targets or the CI/release story). Document the chosen policy and how a developer opts into a native build.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 aarch64 builds are configured to use modern ARMv8.2+ instructions (at minimum dotprod-capable tuning) on Apple Silicon, via an explicit, reviewed policy rather than the bare default baseline
- [ ] #2 The policy preserves portability: generic aarch64 (CI/Linux/older Apple Silicon) and any distributable build path still build and run correctly, with the trade-off documented
- [ ] #3 A measured M3 before/after comparison is recorded (NPS or the standard bench) using controlled same-machine measurement per the repo benchmarking discipline; no cross-session comparison is used
- [ ] #4 The chosen aarch64 build policy and the developer opt-in for a native build are documented (config comment and/or docs)
<!-- AC:END -->
