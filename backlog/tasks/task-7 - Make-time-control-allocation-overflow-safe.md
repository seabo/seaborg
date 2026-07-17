---
id: TASK-7
title: Make time-control allocation overflow safe
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:13'
labels:
  - search
  - uci
  - time
dependencies: []
references:
  - engine/src/time.rs
  - engine/src/engine.rs
priority: high
type: bug
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ordinary timed searches can panic after move 40 or underflow when the per-move allocation is below the safety buffer. Time allocation must remain bounded and meaningful for late games, short clocks, increments, and moves-to-go controls.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Timed search allocation does not panic or wrap for move numbers above the average game length
- [x] #2 Allocations at very low remaining time saturate safely instead of underflowing
- [x] #3 Large protocol time values are handled without lossy narrowing
- [x] #4 Tests cover late-game move numbers, sub-buffer clocks, increments, and explicit moves-to-go values
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Represent UCI clock, increment, moves-to-go, and move-time values with explicit u64 widths through parsing and search-limit conversion.
2. Make estimated-move and allocation arithmetic saturating and guard zero moves-to-go while preserving a minimum late-game horizon.
3. Add focused unit tests for late games, sub-buffer clocks, increments, explicit moves-to-go, and values above u32::MAX.
4. Run formatting and the full Rust workspace test suite, commit the implementation, and record an immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented explicit u64 protocol timing values and saturating allocation arithmetic. Late-game estimates use saturating subtraction with the existing 20-move floor; explicit movestogo values are honored, with zero safely treated as one. Added focused allocation and parser regression tests.

Verification: cargo fmt --check passed; focused time and UCI tests passed (7 tests). cargo test --workspace ran 43 engine tests successfully and failed only pre-existing tt::tests::gen_bound at engine/src/tt.rs:34 (`gen < 64`), which is outside TASK-7 and is concurrently addressed by TASK-4.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 19:01
---
Implementation handoff
Branch: task-7-overflow-safe-time-control
Worktree: /Users/seabo/seaborg-worktrees/task-7-overflow-safe-time-control
Base: ff4276b3b26928053f042776231fc6a9e8d4c163
Implementation target: 27cb863518265f4fd78858e8344434ebe4d439a7
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo test -p engine time::tests: passed (5 tests)
- cargo test -p engine uci::tests: passed (2 tests)
- cargo test --workspace: 43 engine tests passed; one known unrelated failure below
Known failures: tt::tests::gen_bound fails at engine/src/tt.rs:34 because the base implementation supplies a generation >= 64; this is outside TASK-7 and is being addressed by TASK-4.
---

author: @codex
created: 2026-07-17 19:07
---
Review attempt: 1
Reviewed branch: task-7-overflow-safe-time-control
Reviewed implementation: 27cb863518265f4fd78858e8344434ebe4d439a7
Verdict: approved

All acceptance criteria are objectively verified. The base-to-target diff is task-scoped, the implementation target descends from the recorded base, and the post-target handoff commit contains only TASK-7 metadata.

Verification:
- cargo fmt --check: passed
- cargo test -p engine time::tests: 5 passed
- cargo test -p engine uci::tests: 2 passed
- u64::MAX Duration/Instant deadline check: passed
- cargo test --workspace: TASK-7 tests passed; unchanged pre-existing tt::tests::gen_bound failure at engine/src/tt.rs:34
---

author: @codex
created: 2026-07-17 19:12
---
Merged to master as cbb8d06cba3fea9f64246f93d14da7cb17b29826. Post-merge verification: cargo fmt --check passed; cargo test --workspace passed.
---

author: @codex
created: 2026-07-17 19:13
---
Correction: TASK-7's actual merge commit is f78173d (Merge task-7-overflow-safe-time-control). The earlier cbb8d06 reference was the current master tip after a concurrent TASK-6 lifecycle update.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Made UCI timing values explicitly u64 end to end and made late-game, low-clock, increment, and moves-to-go allocation arithmetic saturating. Verified with cargo fmt --check, cargo test -p engine time::tests (5 passed), cargo test -p engine uci::tests (2 passed), an extreme u64::MAX Instant deadline check, and cargo test --workspace (all TASK-7 coverage passed; only the unchanged pre-existing tt::tests::gen_bound failure remains).
<!-- SECTION:FINAL_SUMMARY:END -->
