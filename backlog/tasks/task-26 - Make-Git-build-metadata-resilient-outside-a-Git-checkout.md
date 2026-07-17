---
id: TASK-26
title: Make Git build metadata resilient outside a Git checkout
status: To Do
assignee: []
created_date: '2026-07-17 18:19'
labels:
  - build
  - reliability
dependencies: []
references:
  - build.rs
  - engine/build.rs
priority: medium
type: bug
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The workspace and engine build scripts currently panic when Git cannot be executed, making otherwise valid builds fail in source archives, constrained CI environments, and machines without Git. Make commit metadata resolution robust while preserving useful hashes when repository metadata is available. The separate stdin/EOF finding from the same unwrap audit is already tracked by TASK-8.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The workspace and engine crate build successfully when Git is unavailable or the source directory is not a Git checkout
- [ ] #2 When Git resolves HEAD successfully, both build targets expose the trimmed commit identifier through GIT_HASH
- [ ] #3 When commit metadata cannot be resolved or decoded, both build targets expose a deterministic, non-empty fallback without panicking
- [ ] #4 Regression coverage exercises successful and failed metadata resolution without depending on the developer machine Git state
<!-- AC:END -->
