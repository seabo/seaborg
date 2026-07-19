---
id: TASK-67
title: Centralize Cargo workspace manifest policy
status: To Do
assignee: []
created_date: '2026-07-19 21:18'
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
