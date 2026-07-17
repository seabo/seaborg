---
id: TASK-26
title: Make Git build metadata resilient outside a Git checkout
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-17 18:19'
updated_date: '2026-07-17 18:38'
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
- [x] #1 The workspace and engine crate build successfully when Git is unavailable or the source directory is not a Git checkout
- [x] #2 When Git resolves HEAD successfully, both build targets expose the trimmed commit identifier through GIT_HASH
- [x] #3 When commit metadata cannot be resolved or decoded, both build targets expose a deterministic, non-empty fallback without panicking
- [x] #4 Regression coverage exercises successful and failed metadata resolution without depending on the developer machine Git state
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

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 18:33
---
Implementation handoff
Branch: task-26-resilient-git-metadata
Worktree: /Users/seabo/seaborg-worktrees/task-26-resilient-git-metadata
Base: 46aa66ed04ba91d9526114dd01c5f06fe5668ee4
Implementation target: f6b057b
Resolved findings: none
Verification:
- `cargo fmt --check`: passed
- `cargo test --test build_metadata`: passed (5 tests)
- source archive `cargo check --workspace` without `.git`: passed
- `cargo test --workspace`: 61 passed, 1 ignored, 1 baseline failure across reported suites before stopping
Known failures: `engine::tt::tests::gen_bound` fails `assertion failed: gen < 64`; reproduced with `cargo test -p engine tt::tests::gen_bound -- --exact` at both implementation and base 46aa66e.
---

author: @codex
created: 2026-07-17 18:38
---
Review attempt: 1
Reviewed branch: task-26-resilient-git-metadata
Reviewed implementation: f6b057b
Verdict: approved

All acceptance criteria are proven. The implementation is scoped to shared build metadata resolution and machine-independent regression coverage.

Verification:
- `cargo fmt --check`: passed
- `cargo test --test build_metadata`: 5 passed
- checkout `cargo check --workspace -vv`: engine and seaborg both emitted trimmed HEAD `bb75dbca431fafcb6cf05de91858dcf27e059476`
- `git archive f6b057b` source tree `cargo check --workspace -vv`: passed; engine and seaborg both emitted `unknown`
- `cargo test --workspace`: only `engine::tt::tests::gen_bound` failed; documented baseline failure unchanged at base 46aa66e
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Made both build targets resolve Git metadata through a shared panic-free resolver. Verified the successful path emits the trimmed repository HEAD for engine and seaborg, an exported source tree builds the full workspace and emits the deterministic fallback `unknown` for both targets, and five injected regression tests cover success and failure paths. `cargo fmt --check` and `cargo test --test build_metadata` pass; `cargo test --workspace` has only the pre-existing `engine::tt::tests::gen_bound` failure reproduced at the recorded base.
<!-- SECTION:FINAL_SUMMARY:END -->
