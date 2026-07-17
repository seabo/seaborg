---
id: TASK-20
title: Clarify crate names and public API boundaries
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - architecture
  - api
dependencies: []
references:
  - core/Cargo.toml
  - core/src/lib.rs
  - engine/src/lib.rs
priority: low
type: chore
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The domain crate is named core, which conflicts conceptually with the Rust core crate, and the engine crate publicly exports implementation modules wholesale. Give crates domain-specific names and expose intentional facade APIs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The chess-domain crate no longer uses the ambiguous package and crate name core
- [ ] #2 Downstream imports clearly distinguish board-domain and engine-domain APIs
- [ ] #3 Engine internals are private unless they are part of a documented supported API
- [ ] #4 Workspace binaries, tests, and benchmarks compile against the new public facades
- [ ] #5 The rename and visibility changes are documented for contributors
<!-- AC:END -->
