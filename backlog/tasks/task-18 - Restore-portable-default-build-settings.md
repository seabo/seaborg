---
id: TASK-18
title: Restore portable default build settings
status: Done
assignee: []
created_date: '2026-07-17 17:14'
updated_date: '2026-07-21 14:06'
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
- [x] #1 Default debug and release builds do not require target-cpu=native
- [x] #2 A documented opt-in command or profile remains available for native CPU benchmarking
- [x] #3 Snapshot and release workflows use the portable build unless explicitly overridden
- [x] #4 The portable release binary passes the workspace tests on its build target
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed as superseded (no code changed by this task). The goal — remove the workspace-wide `-C target-cpu=native` default and keep a portable, distributable baseline — was already delivered by TASK-69.5 (commit 1f54c22), which landed after TASK-18 was filed. Verified against master (645fa9c): AC#1 .cargo/config.toml sets `target-cpu=x86-64-v2` for x86_64 and the toolchain default baseline elsewhere, with no native flag in any default build; AC#2 the native opt-in `RUSTFLAGS=\"-C target-cpu=native\" cargo build` is documented in the config header comment and used by docs/strength-testing.md; AC#3 snapshot.sh runs `cargo build --release` (portable via config) and CI empties RUSTFLAGS to the portable toolchain default, while an env RUSTFLAGS still overrides for a deliberate native build; AC#4 CI builds and tests the portable configuration on every push to master. Per human direction, marking Done with this note in the absence of a Cancelled status.
<!-- SECTION:FINAL_SUMMARY:END -->
