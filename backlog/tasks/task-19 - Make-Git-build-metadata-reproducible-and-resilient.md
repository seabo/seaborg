---
id: TASK-19
title: Make Git build metadata reproducible and resilient
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-19 14:24'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make build_metadata.rs the single authoritative source: add an emit() entry point that resolves the revision and prints every cargo directive, so each build script is a one-line call.
2. Honor a SEABORG_GIT_HASH environment override (with rerun-if-env-changed) so source archives, distro packaging, and reproducible builds can pin the revision without Git.
3. Emit rerun-if-changed for the resolved git dir HEAD, the loose ref HEAD points at, and packed-refs, discovered via 'git rev-parse --git-dir'/'--git-path' so linked worktrees and gitdir files work. Without these, no rerun-if-changed is emitted at all today and the embedded hash goes stale after a commit.
4. Delete engine/build.rs. Its GIT_HASH is never read by the engine crate, and its '#[path = "../build_metadata.rs"]' escape puts a file outside the engine package into its build, so 'cargo package' on engine cannot work. Removing it eliminates the duplication and leaves the binary crate as the one place that embeds revision metadata, fed into engine through EngineInfo.
5. Keep resolve_git_hash pure and unit-tested; add tests for the env override precedence, HEAD symref/detached parsing, and the watch-path set.
6. Run cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace.
<!-- SECTION:PLAN:END -->
