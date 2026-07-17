---
id: TASK-6
title: Make fixed-capacity move lists memory safe
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:12'
labels:
  - safety
  - movegen
dependencies: []
references:
  - core/src/movelist.rs
priority: high
type: bug
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The safe MoveList push path performs an unchecked write after only a debug assertion. Overflow must have deterministic safe behavior while preserving the fixed-capacity hot-path design used by move generation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Calling the safe push API at or beyond capacity cannot write out of bounds in any build profile
- [x] #2 Overflow behavior is explicit and consistent for HotArrayVec and ArrayVec-backed move lists
- [x] #3 Tests exercise exact-capacity and over-capacity insertion in debug and release-compatible code
- [x] #4 Normal legal move generation retains all generated moves
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a capacity guard to HotArrayVec's safe push path so overflow is ignored consistently with ArrayVec without changing fixed-capacity storage.
2. Document the shared overflow contract and add boundary tests for exact-capacity and over-capacity insertion on both implementations.
3. Add a legal move-generation regression assertion, run formatting and workspace tests, then commit the immutable implementation and review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Guarded HotArrayVec insertion before its unchecked write and documented fixed-capacity overflow as ignored, matching ArrayVec. Added exact/over-capacity coverage for both list implementations and a 20-legal-move starting-position regression test.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 18:59
---
Implementation handoff
Branch: task-6-memory-safe-movelists
Worktree: /Users/seabo/seaborg-worktrees/task-6-memory-safe-movelists
Base: ff4276b3b26928053f042776231fc6a9e8d4c163
Implementation target: 64f9abb4798645dbedc4d4c83a84ff2eff4ecd0b
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo test -p core movelist::tests: passed (4 tests)
- cargo test --release -p core movelist::tests: passed (4 tests)
- cargo test --workspace: TASK-6 tests passed; suite had one unrelated baseline failure
Known failures: engine::tt::tests::gen_bound fails its unchanged debug assertion for generation 64; isolated rerun reproduces the master-branch test/assertion mismatch.
---

author: @codex-review
created: 2026-07-17 19:07
---
Review attempt: 1
Reviewed branch: task-6-memory-safe-movelists
Reviewed implementation: 64f9abb4798645dbedc4d4c83a84ff2eff4ecd0b
Verdict: approved

Acceptance criteria verified: safe capacity guard prevents out-of-bounds writes; both fixed-capacity backends explicitly ignore overflow; exact/over-capacity tests pass in debug and release; starting-position generation retains all 20 legal moves.

Verification:
- cargo fmt --check: passed
- cargo test -p core movelist::tests: passed (4 tests)
- cargo test --release -p core movelist::tests: passed (4 tests)
- cargo test --workspace: TASK-6 coverage passed; 36 engine tests passed, 1 ignored, with one unrelated pre-existing failure in engine::tt::tests::gen_bound
- cargo test -p engine tt::tests::gen_bound -- --exact: reproduced unchanged baseline failure
- git diff --exit-code ff4276b3b26928053f042776231fc6a9e8d4c163..64f9abb4798645dbedc4d4c83a84ff2eff4ecd0b -- engine/src/tt.rs: passed (file unchanged)
---

author: @codex
created: 2026-07-17 19:12
---
Merged approved task branch task-6-memory-safe-movelists into master at f78173d403ca4e25c4f181cdf924004f6c6171c9. Post-merge verification: cargo fmt --check passed; cargo test -p core movelist::tests passed (4 tests); cargo test --release -p core movelist::tests passed (4 tests).
---

author: @codex
created: 2026-07-17 19:12
---
Correction to merge metadata: TASK-6 was merged by commit 887bc59 (Merge task-6-memory-safe-movelists). Commit f78173d is the subsequent concurrent TASK-7 merge and is not TASK-6's merge commit.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Made fixed-capacity move-list overflow deterministic and memory safe by ignoring pushes at capacity in both HotArrayVec and ArrayVec, while preserving the 1024-byte hot-path representation and normal legal move generation. Verified with cargo fmt --check, focused debug and release tests, and the workspace suite; the sole workspace failure is the unchanged pre-existing engine::tt::tests::gen_bound assertion.
<!-- SECTION:FINAL_SUMMARY:END -->
