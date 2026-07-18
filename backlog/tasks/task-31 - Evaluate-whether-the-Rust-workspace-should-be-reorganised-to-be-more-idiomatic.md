---
id: TASK-31
title: Evaluate whether the Rust workspace should be reorganised to be more idiomatic
status: To Do
assignee: []
created_date: '2026-07-17 23:35'
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
