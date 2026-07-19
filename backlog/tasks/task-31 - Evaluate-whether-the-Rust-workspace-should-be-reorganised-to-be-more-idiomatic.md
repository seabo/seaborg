---
id: TASK-31
title: Evaluate whether the Rust workspace should be reorganised to be more idiomatic
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 23:35'
updated_date: '2026-07-19 21:40'
labels:
  - architecture
dependencies: []
modified_files:
  - docs/workspace-layout-assessment.md
  - backlog/tasks/task-67 - Centralize-Cargo-workspace-manifest-policy.md
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
- [x] #1 The current workspace and crate layout is documented, including crate responsibilities and inter-crate dependencies
- [x] #2 Deviations from idiomatic Rust workspace conventions are identified and explained
- [x] #3 Concrete reorganisation recommendations (or a justified no-change conclusion) are provided, each with rationale and rough effort
- [x] #4 Any recommended follow-up restructuring work is captured as separate tasks
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Correct the manifest duplication analysis so it distinguishes universal package-metadata repetition from selective dependency and path repetition.
2. Create a separate follow-up task for explicit resolver selection and workspace manifest inheritance, then map the recommendation to that task accurately.
3. Record resolutions for REV-1-01 and REV-1-02, run the repository-required checks, commit the revised assessment, and hand a new immutable target to independent review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added `docs/workspace-layout-assessment.md` with the package/target inventory, dependency graph, convention analysis, effort-ranked recommendations, and justified no-change conclusion for directory and package splitting. TASK-20 tracks crate naming and API facades; TASK-21 tracks dependency modernization; TASK-67 tracks explicit resolver selection and workspace manifest inheritance.

Resolved REV-1-01 by creating TASK-67 and mapping manifest policy to it while retaining dependency modernization under TASK-21. Resolved REV-1-02 by distinguishing metadata repeated by all three packages from selectively shared dependency declarations and path dependencies present only in the root and engine manifests.

Verification passed: `cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace`.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 21:13
---
Implementation handoff
Branch: task-31-workspace-layout
Worktree: /Users/seabo/seaborg-worktrees/task-31-workspace-layout
Base: c7826f15b267cd89b0c1c02c97b5294f6ec9bf57
Implementation target: bb4e08154a2bdf5753a54d5f9ebf9c88357b5a9f
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed
Known failures: none
---

author: @codex
created: 2026-07-19 21:16
---
Review attempt: 1
Reviewed branch: task-31-workspace-layout
Reviewed implementation: bb4e08154a2bdf5753a54d5f9ebf9c88357b5a9f
Verdict: changes_requested

REV-1-01 [P1] Manifest-policy recommendation has no matching follow-up scope
Location: docs/workspace-layout-assessment.md:98; backlog/tasks/task-21 - Modernize-and-deduplicate-the-dependency-graph.md
Impact: Acceptance criterion #4 is not met. The assessment recommends explicitly selecting the Cargo resolver and adopting workspace inheritance for package metadata and dependencies, but TASK-21 describes dependency upgrades/deduplication only and none of its acceptance criteria require resolver selection, workspace.package inheritance, or workspace.dependencies inheritance. The report therefore claims follow-up coverage that the referenced task does not provide.
Reproduction: Compare docs/workspace-layout-assessment.md lines 98-110 with TASK-21's description and acceptance criteria.
Expected: Capture the resolver and workspace-inheritance work in a separate follow-up task, or explicitly add that scope and objective acceptance criteria to an appropriate existing follow-up; then make the assessment's mapping accurate.

REV-1-02 [P2] Repeated-manifest description overstates the observed layout
Location: docs/workspace-layout-assessment.md:72
Impact: Acceptance criterion #2 requires deviations to be identified and explained accurately, but the statement that all packages repeat path relationships and common dependency declarations is false: core has no path dependency, and dependency sets are only partially shared. This weakens the rationale for the recommendation.
Reproduction: Compare Cargo.toml, core/Cargo.toml, and engine/Cargo.toml. All repeat version, edition, and license; only the root and engine manifests declare internal path dependencies, and their external dependency sets differ.
Expected: Describe the actual duplication precisely, distinguishing universally repeated package metadata from selectively repeated dependency versions and internal path declarations.

Verification:
- git merge-base --is-ancestor c7826f15b267cd89b0c1c02c97b5294f6ec9bf57 bb4e08154a2bdf5753a54d5f9ebf9c88357b5a9f: passed
- git merge-base --is-ancestor bb4e08154a2bdf5753a54d5f9ebf9c88357b5a9f HEAD: passed
- cargo fmt --check: passed
- CARGO_TARGET_DIR=/tmp/seaborg-task31-review-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (45 core tests, 271 engine tests with 2 ignored, 19 integration tests, 1 compile-fail doctest)
- Manifest, crate-root, benchmark, integration-test, example, TASK-20, and TASK-21 inspection: completed
---

author: @codex
created: 2026-07-19 21:21
---
Implementation handoff
Branch: task-31-workspace-layout
Worktree: /Users/seabo/seaborg-worktrees/task-31-workspace-layout
Base: c7826f15b267cd89b0c1c02c97b5294f6ec9bf57
Implementation target: f52a3ffa002fb10d1322ebbaeafd832fd2d34f62
Resolved findings: REV-1-01, REV-1-02
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (45 core tests; 271 engine tests passed, 2 ignored; 19 integration tests; 1 compile-fail doctest)
Known failures: none
---

author: @codex
created: 2026-07-19 21:24
---
Review attempt: 2
Reviewed branch: task-31-workspace-layout
Reviewed implementation: f52a3ffa002fb10d1322ebbaeafd832fd2d34f62
Verdict: approved

Resolved findings verified: REV-1-01, REV-1-02.

Verification:
- git merge-base --is-ancestor c7826f15b267cd89b0c1c02c97b5294f6ec9bf57 f52a3ffa002fb10d1322ebbaeafd832fd2d34f62: passed
- git merge-base --is-ancestor f52a3ffa002fb10d1322ebbaeafd832fd2d34f62 HEAD: passed
- cargo metadata --no-deps --format-version 1 and manifest/crate-root/consumer inspection: passed
- TASK-20, TASK-21, and TASK-67 scope inspection: passed
- cargo fmt --check: passed
- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (45 core tests; 271 engine tests passed, 2 ignored; 19 integration tests; 1 compile-fail doctest)
- Hot-path benchmarks: not applicable; implementation changes documentation and task metadata only.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Documented the three-package Cargo workspace, its acyclic dependency structure, target and module organization, and the principal convention gaps. Recommended focused crate/API and manifest/dependency cleanup while retaining the current directories and package boundaries; follow-up work is captured in TASK-20, TASK-21, and TASK-67. Independently verified against manifests, Cargo metadata, crate roots, consumers, and follow-up task scopes; cargo fmt, clean-target strict Clippy, and all workspace tests passed.
<!-- SECTION:FINAL_SUMMARY:END -->
