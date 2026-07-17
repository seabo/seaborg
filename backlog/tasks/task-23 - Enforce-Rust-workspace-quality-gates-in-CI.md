---
id: TASK-23
title: Enforce Rust workspace quality gates in CI
status: To Do
assignee: []
created_date: '2026-07-17 17:15'
labels:
  - ci
  - quality
dependencies:
  - TASK-4
  - TASK-17
references:
  - AGENTS.md
  - Cargo.toml
priority: medium
type: chore
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The repository has no checked-in automation enforcing formatting, debug workspace tests, or strict lints. Add a reproducible CI workflow after the known TT test contradiction and lint backlog are resolved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CI runs cargo fmt --check on every proposed change
- [ ] #2 CI runs cargo test --workspace in the debug profile and fails on any test failure
- [ ] #3 CI runs cargo clippy --workspace --all-targets --all-features -- -D warnings
- [ ] #4 The workflow uses a pinned or explicitly managed Rust toolchain and dependency cache inputs
- [ ] #5 Contributor documentation states the same local verification commands
<!-- AC:END -->
