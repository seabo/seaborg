---
id: TASK-18
title: Restore portable default build settings
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
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
