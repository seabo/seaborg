---
id: TASK-20
title: Clarify crate names and public API boundaries
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-20 18:02'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Rename the domain crate: package `core` -> `chess` (crate ident `chess`), git mv dir core/ -> chess/. Update workspace members and all path deps (root, engine, lichess).
2. Migrate all `core::`/`chess_core::` imports to `chess::` across engine, lichess, root binary (src/), and benches. Do NOT touch std `core::arch::aarch64::_prefetch` in engine/tt.rs. Drop the now-redundant `chess_core` alias in root Cargo.toml and benches.
3. Engine facade (keep package name `engine`): make internal-only modules private (`game, history, info, killer, ordering, pv_table, see, trace, uci`); keep the supported API public (`engine, ui, perft, search, time, options, eval, tt, score`). Fix any private-in-public leaks by re-exporting genuinely-needed types through the facade. Add crate-root doc comment stating the supported surface.
4. Ensure workspace binaries, tests, and benches compile against the new facades.
5. Document the rename and visibility model for contributors (crate-level //! docs on chess and engine lib.rs; note in docs/workspace-layout-assessment.md).
6. Run required checks: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, cargo test --workspace. Hand off for review.
<!-- SECTION:PLAN:END -->
