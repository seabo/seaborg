---
id: TASK-77
title: Extract the search test module from search.rs into a sibling file
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-22 16:02'
updated_date: '2026-07-22 16:25'
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
- [x] #1 The #[cfg(test)] module is moved out of engine/src/search.rs into a sibling module file, leaving search.rs containing the search logic plus the module declaration
- [x] #2 No test is added, removed, renamed, skipped or otherwise modified: the same 96 #[test] functions exist and run before and after
- [x] #3 cargo test --workspace passes with an unchanged passing-test count, verified against the count on the pre-change commit
- [x] #4 No search logic is moved or altered; the diff consists of the relocation of the test block plus the module declaration and any imports the moved module needs
- [x] #5 cargo fmt --check and cargo clippy --workspace --all-targets --all-features -- -D warnings both pass
- [x] #6 No trait objects, dynamic dispatch, or new abstraction layers are introduced, and no search logic is split into modules
- [x] #7 No strength run is required and none is claimed; the change is behaviour-preserving by construction
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Relocated the inline `#[cfg(test)] mod tests` block from engine/src/search.rs into engine/src/search/tests.rs, replacing it with `#[cfg(test)] mod tests;`. search.rs goes 7221 -> 3762 lines; tests.rs is 3458 lines. The module path crate::search::tests is unchanged, so `use super::*` and the tests' access to search.rs private items resolve identically; no import changes were needed.

Note on the task description's figures: they were stale. On master 5a43da2 search.rs was 7221 lines (not 6301), the test module began at line 3761 (not 657), and it held 113 #[test] functions (not 96). Scope was unchanged; the counts below are against the actual pre-change commit.

Fidelity evidence:
- git diff of search.rs is 1 insertion / 3460 deletions and contains nothing but the removed test block plus `mod tests;`. No search logic touched.
- Normalising away whitespace, braces and commas, the moved block is character-identical to the original (108675 == 108675 chars). The only textual changes are rustfmt reflowing lines that gained four columns from the one-level dedent (e.g. a collapsed trailing comma, a match arm losing redundant braces).
- `cargo test -p engine --lib -- --list` name lists before and after diff clean: 428 names, of which 113 are search::tests::*.
- Workspace passing count unchanged at 629 passed / 0 failed / 2 ignored.

No trait objects, dynamic dispatch or abstraction layers introduced; no search logic split into modules. Behaviour-preserving by construction, so no strength run was needed or claimed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-22 16:17
---
Implementation handoff
Branch: task-77-extract-search-tests
Worktree: /Users/seabo/seaborg-worktrees/task-77-extract-search-tests
Base: 5a43da27e0343b2f0a2374f3afeca1c0147dabaa
Implementation target: 59f1445
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 629 passed / 0 failed / 2 ignored, identical to the pre-change count on 5a43da2
- cargo test -p engine --lib -- --list: 428 test names, diff-clean against the pre-change list
Known failures: none
---

author: @claude
created: 2026-07-22 16:25
---
Review verdict: APPROVED

Attempt: 1
Base: 5a43da27e0343b2f0a2374f3afeca1c0147dabaa
Implementation target (code SHA): 59f1445
Branch: task-77-extract-search-tests
Worktree: /Users/seabo/seaborg-worktrees/task-77-extract-search-tests

Immutability: 59f1445 is an ancestor of the branch tip; the only later commit (e1d582e) touches the task file alone. No implementation file changes after the target.

Scope: the base-to-target diff touches exactly engine/src/search.rs, engine/src/search/tests.rs, and the task file. Nothing unrelated.

Fidelity verification (done independently, not taken from the handoff):
- git diff of search.rs across the target adds exactly one line, `mod tests;`, and removes the 3460-line test block. No search logic line is touched.
- Extracted the original block from 5a43da2, dedented one level, and diffed it against engine/src/search/tests.rs: 10 hunks, every one a rustfmt reflow caused by the four columns freed by the dedent (joined continuation lines, one match arm shedding redundant braces, one .expect() re-wrapped). No semantic difference.
- No trait objects, dyn dispatch, or new abstraction layers; no search logic split into modules. No #[allow] added.

Checks run on the target:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, zero warnings, confirmed with a fresh CARGO_TARGET_DIR so no cached lint result is load-bearing
- cargo test --workspace: 629 passed / 0 failed; identical aggregate on base 5a43da2
- cargo test -p engine --lib -- --list: 428 names before and after, diff-clean, 113 of them search::tests::*

Benchmarks: not run. The entire diff is inside #[cfg(test)], so no code reaching the release binary or the bench targets changed; there is no hot path for a movegen or perft delta to appear on.

Note for the record: the task description's figures (6301 lines, tests at 657, 96 tests) were stale. The actual pre-change state was 7221 lines, test module at 3761, 113 tests. The implementer flagged this and the scope was unaffected.

Acceptance criteria 1-7: all checked, each proven by the evidence above.

Verdict: Ready to Merge. Approved code target 59f1445.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Relocated the inline `#[cfg(test)] mod tests` block from engine/src/search.rs into engine/src/search/tests.rs, leaving `#[cfg(test)] mod tests;` behind. search.rs drops 7221 -> 3762 lines; the module path crate::search::tests is unchanged so `use super::*` and private-item access still resolve. Verified as a pure relocation: search.rs's diff is one added line (`mod tests;`) plus the removed block, and diffing the original block dedented one level against tests.rs yields only rustfmt reflows of lines that gained four columns (10 hunks, all line-joining or redundant-brace removal, no semantic change). cargo test -p engine --lib -- --list is byte-identical before and after (428 names, 113 under search::tests::), and cargo test --workspace passes 629/629 on both 5a43da2 and the target. cargo fmt --check clean; cargo clippy --workspace --all-targets --all-features -- -D warnings clean with a fresh CARGO_TARGET_DIR.
<!-- SECTION:FINAL_SUMMARY:END -->
