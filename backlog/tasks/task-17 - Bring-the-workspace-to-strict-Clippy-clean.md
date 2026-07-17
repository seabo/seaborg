---
id: TASK-17
title: Bring the workspace to strict Clippy clean
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - quality
  - rust
dependencies: []
references:
  - Cargo.toml
priority: medium
type: chore
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Strict Clippy currently fails and normal Clippy reports a large warning backlog across core, engine, the binary, and build scripts. Resolve or narrowly justify warnings so lint failures can become an enforced quality gate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cargo clippy --workspace --all-targets --all-features -- -D warnings passes
- [ ] #2 Any lint allowance is local and documents why the warned construct is required
- [ ] #3 Behavioral changes made during cleanup have focused regression coverage
- [ ] #4 cargo fmt --check and cargo test --workspace continue to pass
<!-- AC:END -->
