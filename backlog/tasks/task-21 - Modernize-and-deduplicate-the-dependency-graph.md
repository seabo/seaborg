---
id: TASK-21
title: Modernize and deduplicate the dependency graph
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - dependencies
  - maintenance
dependencies: []
references:
  - Cargo.toml
  - core/Cargo.toml
  - engine/Cargo.toml
priority: low
type: chore
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The workspace carries duplicate major generations and several legacy direct dependency lines, including two rand versions. Update direct dependencies deliberately and remove dependencies that no longer justify their maintenance surface.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Direct dependencies use supported versions compatible with the workspace toolchain
- [ ] #2 The workspace does not depend on multiple rand major versions through its own direct dependency choices
- [ ] #3 Unused direct dependencies are removed
- [ ] #4 cargo tree --workspace --duplicates is reviewed and remaining duplicates are documented or transitive-only
- [ ] #5 Workspace tests and benchmarks pass after dependency updates
<!-- AC:END -->
