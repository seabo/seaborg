---
id: TASK-19
title: Make Git build metadata reproducible and resilient
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
labels:
  - build
  - metadata
dependencies: []
references:
  - build.rs
  - engine/build.rs
priority: low
type: chore
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Duplicated build scripts assume Git is installed and the source is a checkout, unwrap command and UTF-8 failures, and embed raw command output. Consolidate commit metadata with deterministic fallbacks and correct rebuild triggers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Building from a source archive or environment without Git succeeds
- [ ] #2 The embedded revision is trimmed and has a documented fallback value
- [ ] #3 Cargo reruns metadata generation when the relevant revision state changes
- [ ] #4 Duplicate build-script logic is removed or shared from one authoritative location
- [ ] #5 Package, workspace, and engine builds expose consistent version metadata
<!-- AC:END -->
