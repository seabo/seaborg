---
id: TASK-18
title: Restore portable default build settings
status: Needs Human
assignee: []
created_date: '2026-07-17 17:14'
updated_date: '2026-07-21 13:58'
labels:
  - build
  - portability
dependencies: []
references:
  - .cargo/config.toml
  - snapshot.sh
priority: low
type: chore
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Workspace-wide target-cpu=native makes ordinary release artifacts dependent on the build machine CPU. Keep portable defaults while retaining an explicit path for local native benchmarking.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Default debug and release builds do not require target-cpu=native
- [ ] #2 A documented opt-in command or profile remains available for native CPU benchmarking
- [ ] #3 Snapshot and release workflows use the portable build unless explicitly overridden
- [ ] #4 The portable release binary passes the workspace tests on its build target
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Superseded by TASK-69.5 (commit 1f54c22, "AVX2 NNUE inference with runtime dispatch and a distributable build"), which landed after TASK-18 was filed. That change already removed the workspace-wide `-C target-cpu=native` default and installed a portable per-architecture baseline.

State of the acceptance criteria on master (645fa9c):
- AC#1 (defaults do not require target-cpu=native): met. .cargo/config.toml sets `-C target-cpu=x86-64-v2` for x86_64 and keeps the toolchain default baseline elsewhere; no native flag in any default build.
- AC#2 (documented native opt-in remains): met. The opt-in `RUSTFLAGS="-C target-cpu=native" cargo build` is documented in the .cargo/config.toml header comment and used in docs/strength-testing.md. (Not surfaced in README's Building section, which is the only discoverability gap.)
- AC#3 (snapshot/release use portable unless overridden): met. snapshot.sh runs `cargo build --release` (portable via config.toml); CI empties RUSTFLAGS to the toolchain portable default; an env RUSTFLAGS still overrides config.toml for a deliberate native build.
- AC#4 (portable release passes workspace tests): not independently re-verified in this session; CI already builds and tests the portable configuration on every push.

No target-cpu=native default remains to 'restore', so there is no in-scope implementation work left. Escalating for a human scope decision rather than fabricating a change: recommend cancelling/closing TASK-18 as superseded, or re-scoping it to the sole remaining discoverability nit (document the portable-default / native opt-in in README's Building section). No code was changed on this branch.
<!-- SECTION:NOTES:END -->
