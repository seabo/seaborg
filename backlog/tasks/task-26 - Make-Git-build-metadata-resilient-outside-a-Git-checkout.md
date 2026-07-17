---
id: TASK-26
title: Make Git build metadata resilient outside a Git checkout
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 18:19'
updated_date: '2026-07-17 18:33'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extract one shared build-metadata resolver used by the workspace and engine build scripts.
2. Resolve `git rev-parse HEAD` only on successful command status and valid trimmed UTF-8, otherwise emit a documented deterministic non-empty fallback.
3. Add machine-independent regression tests with injected command results covering success, missing Git/non-checkout failure, unsuccessful status, invalid UTF-8, and empty output.
4. Run formatting and the full workspace test suite, then commit the implementation and record the immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a shared build metadata resolver for both build scripts. Successful Git output is UTF-8 decoded and trimmed; missing Git, unsuccessful commands, invalid UTF-8, and empty output use the documented deterministic fallback `unknown`. Added injected regression coverage for each resolution path.

Verification note: the source-archive workspace check succeeds without a `.git` directory. The full workspace test suite has one unrelated baseline failure, `engine::tt::tests::gen_bound`, which reproduces at the untouched base commit.
<!-- SECTION:NOTES:END -->
