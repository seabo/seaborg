---
id: TASK-20
title: Clarify crate names and public API boundaries
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-20 18:18'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation summary
- Renamed domain crate `core` -> `chess` (git mv core/ chess/, package name, workspace members, and all path deps). Dropped the root/bench `chess_core` alias; consumers now import `chess::` directly. Preserved the std `core::arch::aarch64` reference in a tt.rs comment (not the workspace crate).
- Engine facade: kept `pub` only the externally-consumed modules (eval, options, perft, score, search, time, tt, ui). Re-exported the UCI entry point at the crate root (`pub use engine::{launch, EngineInfo}`) and made the `engine` submodule private, removing the awkward `engine::engine` double-name and the `::engine` disambiguation in the binary. Made game/history/info/killer/ordering/pv_table/see/trace/uci private.
- Privatising those modules unmasked pre-existing dead code (the old blanket `pub` suppressed dead_code because every item was externally reachable). Resolved at the source rather than with #[allow]: removed unused `Game::reset_to`, `Butterfly::inc`, `Killer::slots`, and the never-constructed `uci::Error` variants `ExpectedBool`/`InvalidPosition`/`InvalidMove`; gated the test-only bounds-checked accessors `Butterfly::get`/`HistoryTable::get` behind #[cfg(test)]. No exhaustive match or Display/From impl referenced the removed variants.
- Fixed a compile_fail doctest in chess/src/position/square.rs that imported `core::position::Square` (now std core) so it fails for the intended private-field reason again.
- Docs (AC5): added crate-root //! docs to chess and engine describing the supported surface and rename rationale; updated docs/workspace-layout-assessment.md (layout table, dependency diagram, deviations/recommendation/result) to the post-rename state. TASK-67 (manifest policy) noted as still open.

Design decisions confirmed with the user: domain crate named `chess` (concise, no separators, unambiguous with std core); `engine` package name kept (milder ambiguity) with facade-only changes.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 18:18
---
Implementation handoff
Branch: task-20-clarify-crate-names
Worktree: /Users/seabo/seaborg-worktrees/task-20-clarify-crate-names
Base: ba6aec1d2d2633c672e9945d52864fb09c011140
Implementation target: e06cae895d7613a0269e0b849ba6458e6752489e
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings)
- cargo test --workspace: pass (45 + 291/2-ignored + 68 + 19 unit/integration, all doctests; 0 failed)
- cargo build -p engine --examples: pass
Known failures: none
---
<!-- COMMENTS:END -->
