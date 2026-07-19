---
id: TASK-67
title: Centralize Cargo workspace manifest policy
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 21:18'
updated_date: '2026-07-19 21:50'
labels:
  - architecture
dependencies: []
references:
  - docs/workspace-layout-assessment.md
priority: low
type: chore
ordinal: 85000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make the workspace's Cargo policy explicit and consistent by selecting the resolver deliberately and centralizing genuinely shared package metadata and dependency declarations. This follows the workspace-layout assessment and is separate from dependency upgrades or deduplication.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The workspace manifest explicitly selects the intended Cargo feature resolver, with the choice documented where needed
- [ ] #2 Shared package metadata is declared once under workspace.package and inherited by every applicable member
- [ ] #3 Dependencies shared across member packages are declared under workspace.dependencies and inherited without changing dependency features or runtime behavior
- [ ] #4 Member-specific dependencies and internal path relationships remain explicit where inheritance would obscure ownership
- [ ] #5 Cargo metadata succeeds and all repository-required Rust checks pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add resolver = "2" to [workspace] (explicit; matches edition-2021 default, no behavior change) with a short rationale comment.
2. Add [workspace.package] with version, authors, edition, license; inherit via .workspace = true in root, core, and engine packages.
3. Add [workspace.dependencies] for the genuinely shared crates: rand = "0.10.2" (core dep + engine dev-dep) and separator = "0.4" (root + engine). Inherit via .workspace = true, preserving default features.
4. Keep internal path deps (chess_core, engine, core) and single-member deps explicit (AC#4).
5. Verify: cargo metadata succeeds, cargo fmt --check, clippy -D warnings, cargo test --workspace. Confirm Cargo.lock unchanged (no dependency/feature drift).
<!-- SECTION:PLAN:END -->
