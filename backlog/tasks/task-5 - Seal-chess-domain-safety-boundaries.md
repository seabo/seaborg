---
id: TASK-5
title: Seal chess domain safety boundaries
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:31'
labels:
  - safety
  - core
dependencies: []
references:
  - core/src/position/square.rs
  - core/src/position/board.rs
  - core/src/mov.rs
  - core/src/position/mod.rs
priority: high
type: bug
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Public safe domain types currently allow invalid squares, moves, and positions to reach unchecked indexing and mutation paths. Make invalid state construction explicit and ensure safe APIs cannot cause undefined behavior from caller-controlled values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Invalid square values cannot be constructed through the normal safe API
- [x] #2 Safe Board, Move, and Position operations reject invalid input without undefined behavior in debug or release builds
- [x] #3 Any remaining unchecked operations are private or exposed through an unsafe API with a precise safety contract
- [x] #4 Regression tests cover invalid square, move, and blank-position inputs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Rework public Bitboard-to-Square conversions so empty bitboards cannot produce an invalid Square in any build profile.
2. Migrate all callers to handle the checked result while preserving behavior for proven non-empty iteration paths.
3. Add regression tests for empty conversion/pop and the Board boundary described by REV-1-01.
4. Run focused debug/release tests plus repository-required formatting and workspace tests; commit the rework and create a new immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sealed `Square` behind a private external representation with checked raw conversion and checked arithmetic. Hardened Board placement, Move construction, and pre-mutation Position move validation. Migrated engine consumers to the public square index accessor and added debug/release regression coverage for invalid square, board, move, null-move, and blank-position inputs.

Workspace verification note: `cargo test --workspace` passed all core tests and 36/37 non-ignored engine tests; pre-existing `engine::tt::tests::gen_bound` fails because the test deliberately passes generation 64 to a function with `debug_assert!(gen < 64)`. The same contradictory test and assertion are present at the base commit.

Resolved REV-1-01: `Bitboard::to_square` now returns `Option<Square>` unless exactly one bit is set, and `pop_lsb_and_bit` uses a release-active assertion before constructing a square. Migrated king and SEE callers and added debug/release regressions covering empty and multi-bit conversions, empty direct pop, and safe composition with Board indexing.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 19:03
---
Implementation handoff
Branch: task-5-domain-safety
Worktree: /Users/seabo/seaborg-worktrees/task-5-domain-safety
Base: ff4276b3b26928053f042776231fc6a9e8d4c163
Implementation target: 68dfdba
Resolved findings: none
Verification:
- cargo fmt --all --check: passed
- cargo test -p core --release: passed (22 unit tests, 1 compile-fail doctest)
- cargo test -p engine --lib tt::tests::packed_entry_round_trips: passed
- cargo test --workspace: core passed 21/21; engine passed 36/37 non-ignored tests
Known failures: engine::tt::tests::gen_bound fails at engine/src/tt.rs:34 because the base test passes generation 64 to existing debug_assert!(gen < 64); unchanged from base.
---

author: @codex
created: 2026-07-17 19:07
---
Review attempt: 1\nReviewed branch: task-5-domain-safety\nReviewed implementation: 68dfdba\nVerdict: changes_requested\n\nREV-1-01 [P1] Empty bitboards still construct invalid squares through safe APIs\nLocation: core/src/bb.rs:105-134; core/src/position/board.rs:22-27\nImpact: In release builds, the public safe calls Bitboard::empty().to_square() and Bitboard::empty().pop_lsb_and_bit() construct Square(64). Passing that value to safe Board::piece_at_sq reaches get_unchecked(64), so caller-controlled safe values can still cause undefined behavior. This blocks acceptance criteria 1-4.\nReproduction: Link a small external caller against the release core crate and evaluate Bitboard::empty().to_square().index(); it prints 64. The debug build panics only because to_square uses debug_assert, so behavior differs by profile.\nExpected: No public safe Bitboard conversion may return an invalid Square. Empty input must be represented by Option/Result or rejected with a non-debug assertion before constructing Square, and regression tests must cover the release behavior and the Board boundary.\n\nVerification:\n- cargo fmt --all --check: passed\n- cargo test -p core: passed (22 tests, 1 compile-fail doctest)\n- cargo test -p core --release: passed (22 tests, 1 compile-fail doctest), but lacks the empty-bitboard regression\n- external release reproduction: safe API produced Square(64)
---

author: @codex
created: 2026-07-17 19:08
---
Formatting correction for review comment #2:

Review attempt: 1
Reviewed branch: task-5-domain-safety
Reviewed implementation: 68dfdba
Verdict: changes_requested

REV-1-01 [P1] Empty bitboards still construct invalid squares through safe APIs
Location: core/src/bb.rs:105-134; core/src/position/board.rs:22-27
Impact: In release builds, the public safe calls Bitboard::empty().to_square() and Bitboard::empty().pop_lsb_and_bit() construct Square(64). Passing that value to safe Board::piece_at_sq reaches get_unchecked(64), so caller-controlled safe values can still cause undefined behavior. This blocks acceptance criteria 1-4.
Reproduction: Link a small external caller against the release core crate and evaluate Bitboard::empty().to_square().index(); it prints 64. The debug build panics only because to_square uses debug_assert, so behavior differs by profile.
Expected: No public safe Bitboard conversion may return an invalid Square. Empty input must be represented by Option/Result or rejected with a non-debug assertion before constructing Square, and regression tests must cover the release behavior and the Board boundary.

Verification:
- cargo fmt --all --check: passed
- cargo test -p core: passed (22 tests, 1 compile-fail doctest)
- cargo test -p core --release: passed (22 tests, 1 compile-fail doctest), but lacks the empty-bitboard regression
- external release reproduction: safe API produced Square(64)
---

author: @codex
created: 2026-07-17 19:26
---
Implementation handoff
Branch: task-5-domain-safety
Worktree: /Users/seabo/seaborg-worktrees/task-5-domain-safety
Base: ff4276b3b26928053f042776231fc6a9e8d4c163
Implementation target: 91c1dc33cacfdb274ff153f66fac743701574792
Resolved findings: REV-1-01
Verification:
- cargo fmt --all --check: passed
- cargo test -p core: passed (25 unit tests, 1 compile-fail doctest)
- cargo test -p core --release: passed (25 unit tests, 1 compile-fail doctest)
- cargo test --workspace: core passed 25/25; engine passed 36/37 non-ignored tests
Known failures: engine::tt::tests::gen_bound fails at engine/src/tt.rs:34 because the test passes generation 64 to existing debug_assert!(gen < 64); unchanged from the recorded base and prior implementation handoff.
---

author: @codex
created: 2026-07-17 19:29
---
Review attempt: 2
Reviewed branch: task-5-domain-safety
Reviewed implementation: 91c1dc33cacfdb274ff153f66fac743701574792
Verdict: approved

All acceptance criteria are proven. REV-1-01 is resolved: singleton conversion is checked, empty direct pop uses a release-active assertion, and safe composition cannot produce Square(64). The implementation target descends from the recorded base, and the only later commit before approval contains task handoff metadata.

Verification:
- cargo fmt --all --check: passed
- cargo test -p core: passed (25 unit tests, 1 compile-fail doctest)
- cargo test -p core --release: passed (25 unit tests, 1 compile-fail doctest)
- cargo test --workspace: core passed; engine passed 36/37 non-ignored tests, with only unchanged baseline engine::tt::tests::gen_bound failing
---

author: @codex
created: 2026-07-17 19:31
---
Merged to master at 557c1cc96df4365297ec56f14aad11413426521d. Post-merge verification: cargo fmt --all --check passed; cargo test -p core --release passed (31 unit tests and 1 compile-fail doctest); cargo test --workspace passed.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Sealed Square construction and arithmetic, hardened Board, Move, Position, and Bitboard boundaries, and added invalid-input regressions. Review verified implementation 91c1dc33cacfdb274ff153f66fac743701574792 with cargo fmt --all --check, cargo test -p core, and cargo test -p core --release; cargo test --workspace has only the unchanged baseline engine::tt::tests::gen_bound failure.

Merged to master at 557c1cc96df4365297ec56f14aad11413426521d; post-merge formatting, release core tests, and full workspace tests passed.
<!-- SECTION:FINAL_SUMMARY:END -->
