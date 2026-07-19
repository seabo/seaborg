---
id: TASK-21
title: Modernize and deduplicate the dependency graph
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-19 21:29'
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
- [x] #1 Direct dependencies use supported versions compatible with the workspace toolchain
- [x] #2 The workspace does not depend on multiple rand major versions through its own direct dependency choices
- [x] #3 Unused direct dependencies are removed
- [x] #4 cargo tree --workspace --duplicates is reviewed and remaining duplicates are documented or transitive-only
- [x] #5 Workspace tests and benchmarks pass after dependency updates
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inventory direct dependency usage and baseline duplicate generations across all workspace manifests.
2. Remove unused direct dependencies, upgrade supported direct crates to current compatible releases, and align every direct rand consumer on one major version with the necessary API migrations.
3. Regenerate the lockfile, compile all targets, and resolve dependency-upgrade API or lint fallout without expanding product behavior.
4. Review and document remaining cargo-tree duplicates, run workspace tests plus benchmark smoke/build coverage and all repository-required gates, then commit an immutable implementation and hand off for independent review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Updated direct dependencies to current releases compatible with rustc 1.97.1, including clap 4.6.2, criterion 0.8.2, bitflags 2.13.1, rand 0.10.2, simple_logger 5.2.0, open 5.4.0, unicode-segmentation 1.13.3, and crossbeam-channel 0.5.16. Removed unused core separator/log and engine log declarations. Aliased the root package dependency as chess_core so clap 4 derive expansion can address Rust core without collision. Migrated rand, bitflags, and Criterion APIs. cargo tree --workspace --duplicates now reports only syn 2.0.119 and 3.0.0; both are transitive proc-macro dependencies through clap/zerocopy and serde/criterion respectively.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 21:15
---
Implementation handoff
Branch: task-21-dependency-graph
Worktree: /Users/seabo/seaborg-worktrees/task-21-dependency-graph
Base: c7826f15b267cd89b0c1c02c97b5294f6ec9bf57
Implementation target: ce88829883ddc6add2fb484cb8602c040853fff1
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (335 passed, 2 ignored)
- cargo bench --workspace --no-run: passed
- cargo bench --workspace -- --test: passed (all benchmark targets reported Success)
- cargo tree --workspace --duplicates: reviewed; only transitive syn 2/3 remain
Known failures: none
---

author: @codex-reviewer
created: 2026-07-19 21:20
---
Review attempt: 1
Reviewed branch: task-21-dependency-graph
Reviewed implementation: ce88829883ddc6add2fb484cb8602c040853fff1
Verdict: approved

All acceptance criteria are objectively satisfied. The base-to-target diff is task-scoped, the target is immutable beneath a task-only handoff commit, direct rand consumers use 0.10.2, unused direct dependencies were removed, and the only remaining duplicate generations are transitive syn 2.0.119/3.0.0.

Verification:
- cargo fmt --check: passed
- CARGO_TARGET_DIR=/tmp/seaborg-task21-review-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings: passed from a fresh target directory
- cargo test --workspace: passed (335 passed, 2 ignored)
- cargo bench --workspace -- --test: passed; every benchmark target reported Success
- cargo tree --workspace --duplicates: reviewed; only transitive syn 2/3 remain
---

author: @codex-merge
created: 2026-07-19 21:29
---
Merge completed
Integrated merge: ec634e24318fb5f5057421733ac81540cc00612b
Primary base: ddf871fdbe313ae04a195745b019e50f0e0b2d59

Verification on integrated result:
- cargo fmt --check: passed
- CARGO_TARGET_DIR=/tmp/seaborg-task21-merge-clippy-ec634e2 cargo clippy --workspace --all-targets --all-features -- -D warnings: passed from a fresh target directory
- cargo test --workspace: passed (337 passed, 2 ignored)
- cargo bench --workspace -- --test: passed; every benchmark target reported Success

Overlap: TASK-64.14 also changed benches/search.rs; the clean merge preserves its static_eval benchmark update alongside TASK-21's Criterion and crate-import migrations.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Modernized and consolidated direct dependencies, removed unused declarations, and migrated affected APIs. Independently verified implementation ce88829883ddc6add2fb484cb8602c040853fff1 with cargo fmt --check, clean-target strict Clippy, cargo test --workspace, cargo bench --workspace -- --test, and cargo tree --workspace --duplicates; only transitive syn 2/3 duplication remains.
<!-- SECTION:FINAL_SUMMARY:END -->
