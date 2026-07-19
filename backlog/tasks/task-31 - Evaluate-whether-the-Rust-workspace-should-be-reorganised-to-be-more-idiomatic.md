---
id: TASK-31
title: Evaluate whether the Rust workspace should be reorganised to be more idiomatic
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 23:35'
updated_date: '2026-07-19 21:10'
labels:
  - architecture
dependencies: []
priority: low
type: chore
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Review the overall layout of the Cargo workspace (crate boundaries, module organisation, directory structure, naming, and dependency wiring) and assess whether it follows idiomatic Rust conventions. Produce recommendations for any restructuring that would improve clarity and maintainability, or conclude that the current organisation is already idiomatic. This is an investigation/proposal task; it need not carry out the reorganisation itself.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The current workspace and crate layout is documented, including crate responsibilities and inter-crate dependencies
- [ ] #2 Deviations from idiomatic Rust workspace conventions are identified and explained
- [ ] #3 Concrete reorganisation recommendations (or a justified no-change conclusion) are provided, each with rationale and rough effort
- [ ] #4 Any recommended follow-up restructuring work is captured as separate tasks
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inventory workspace packages, targets, module boundaries, and dependency direction from manifests and source entry points.
2. Compare the observed layout with idiomatic Cargo workspace and Rust naming conventions, distinguishing structural issues from cosmetic preferences.
3. Add a durable architecture assessment with concrete recommendations, rationale, effort estimates, and mappings to existing or new follow-up tasks.
4. Run repository-required checks, record evidence, commit the immutable implementation, and hand the task to independent review.
<!-- SECTION:PLAN:END -->
