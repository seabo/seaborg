---
id: TASK-77
title: Extract the search test module from search.rs into a sibling file
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-22 16:02'
updated_date: '2026-07-22 16:12'
labels:
  - search
  - hygiene
  - refactor
dependencies: []
references:
  - engine/src/search.rs
priority: medium
type: chore
ordinal: 132000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
engine/src/search.rs is 6301 lines, but only ~650 of those are search logic: the inline `#[cfg(test)]` module begins at line 657 and runs to the end of the file, carrying 96 `#[test]` functions (~5644 lines). The file reads as though the search is sprawling when in fact the test suite is simply co-located with it.

Move that test module into a sibling module file so the working file reflects the size of the logic. This is deliberately a pure relocation, not a logic refactor.

Why this is safe. Test code is not compiled into the release binary, so relocating it has exactly zero runtime or benchmark impact. Rust also inlines freely across modules within a crate, so module boundaries per se cost nothing.

Explicitly out of scope, to avoid the failure mode this task is designed to dodge: do NOT split the search logic itself into modules, and do NOT introduce trait objects, dynamic dispatch, or any indirection or abstraction layer that could inhibit inlining on the hot path. The search logic at ~650 lines does not warrant carving up, and any such change would need its own benchmark evidence. If a future logic split is wanted, the natural seam is new subsystems (for example the Lazy SMP search-team lifecycle owner in TASK-64.16.2), not today's node search.

Sequencing. Run this only when no other search.rs task is in flight: any concurrent strength or pruning task will be adding tests into the very module being relocated, which turns a mechanical move into a merge conflict.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The #[cfg(test)] module is moved out of engine/src/search.rs into a sibling module file, leaving search.rs containing the search logic plus the module declaration
- [ ] #2 No test is added, removed, renamed, skipped or otherwise modified: the same 96 #[test] functions exist and run before and after
- [ ] #3 cargo test --workspace passes with an unchanged passing-test count, verified against the count on the pre-change commit
- [ ] #4 No search logic is moved or altered; the diff consists of the relocation of the test block plus the module declaration and any imports the moved module needs
- [ ] #5 cargo fmt --check and cargo clippy --workspace --all-targets --all-features -- -D warnings both pass
- [ ] #6 No trait objects, dynamic dispatch, or new abstraction layers are introduced, and no search logic is split into modules
- [ ] #7 No strength run is required and none is claimed; the change is behaviour-preserving by construction
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Create task branch/worktree from master (done).
2. Move the inline `#[cfg(test)] mod tests` block (engine/src/search.rs lines 3761-7221, 113 #[test] fns) verbatim into engine/src/search/tests.rs, dedenting one level.
3. Replace it in search.rs with `#[cfg(test)]\nmod tests;`. Module path crate::search::tests is unchanged, so `use super::*` and access to search.rs private items still resolve identically.
4. Record pre-change `cargo test --workspace` passing count, then confirm the post-change count matches.
5. Run cargo fmt --check, clippy -D warnings, cargo test --workspace; hand off to review.
Note: the task description's line/test counts (6301 lines, tests at 657, 96 tests) are stale; master today has search.rs at 7221 lines with the test module at 3761 and 113 tests. Scope is unchanged.
<!-- SECTION:PLAN:END -->
