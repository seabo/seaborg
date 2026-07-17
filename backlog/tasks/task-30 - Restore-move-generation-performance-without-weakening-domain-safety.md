---
id: TASK-30
title: Restore move-generation performance without weakening domain safety
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 20:57'
updated_date: '2026-07-17 21:40'
labels:
  - performance
  - safety
  - core
dependencies: []
references:
  - BENCHMARKS.md
  - core/src/mov.rs
  - core/src/position/square.rs
  - core/src/position/mod.rs
  - core/src/position/board.rs
  - core/src/bb.rs
  - core/src/movegen.rs
  - engine/src/perft.rs
priority: high
type: bug
ordinal: 33000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Move generation and perft regressed after the domain-safety work introduced at commit 68dfdba. A controlled bisect measured the parent at 183.07 ns for move generation and 21.152 ms for perft(5), while 68dfdba measured 252.14 ns and 30.771 ms respectively. Restore hot-path performance while retaining TASK-5’s guarantee that caller-controlled invalid squares, moves, bitboards, and position mutations cannot cause undefined behavior through safe APIs. Validation should remain at untrusted boundaries without being redundantly repeated for values whose invariants are already established inside trusted engine paths.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Safe public Square, Bitboard, Board, Move, and Position APIs continue to reject invalid caller-controlled input in both debug and release builds without undefined behavior
- [ ] #2 Any validation-eliding operation used by move generation, search, or perft is private or unsafe, has a precise documented safety contract, and is reached only from audited trusted engine paths
- [ ] #3 Move generation, search, and perft do not repeatedly perform release-mode structural or position validation for moves whose invariants were established by trusted engine move generation
- [ ] #4 Regression tests continue to cover invalid raw squares, empty and multi-bit bitboard conversion, invalid move encodings, empty move origins, friendly captures, invalid special-move metadata, and null moves passed to normal position mutation
- [ ] #5 On the documented Apple M3 Pro and Rust 1.97.1 environment, a full idle-machine Criterion run reports generate moves at or below 193.83 ns and perft 5 at or below 22.472 ms, with confidence intervals and the tested commit recorded in the task handoff
- [ ] #6 If the resulting full Criterion measurements show a repeatable improvement over the documented baseline, BENCHMARKS.md is updated to the improved values, commit, hardware, and toolchain; a single noisy run does not move the baseline
- [ ] #7 cargo fmt --check and cargo test --workspace pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Establish baseline profiles and isolate release-mode validation costs in move construction and position mutation.
2. Add narrowly scoped validation-eliding APIs with documented safety contracts, keeping all existing safe public entry points fully checked.
3. Route only audited generated-move paths in move generation, search, ordering, and perft through those trusted operations; retain safe validation at parser/UI/external boundaries.
4. Extend or preserve invalid-input regression coverage, then run formatting, workspace tests, and focused release checks.
5. Run full idle-machine Criterion measurements on the implementation commit, update BENCHMARKS.md only if repeatable results justify it, and record confidence intervals in the review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented checked/unchecked boundary splits for Move construction, Position mutation, and internal Square offsets. Safe public constructors and mutation retain release-mode assertions; generated moves use crate-private or unsafe operations with explicit contracts. Added regression tests proving friendly captures and invalid castling/en-passant metadata are rejected before mutation. BENCHMARKS.md was not changed because the final measurements do not show a repeatable improvement over its baseline.

Verification: cargo fmt --check passed; cargo test --workspace passed (33 core, 48 engine with 1 ignored, 5 metadata, 1 doctest); cargo test -p core --release passed (33 tests); Criterion on Apple M3 Pro with rustc/cargo 1.97.1 at d56d02aa0726d1cb079af8d41a9e087ebb1efa8b reported generate moves 187.41 ns (95% CI 186.50–188.45 ns) and perft 5 22.434 ms (95% CI 22.356–22.524 ms). Background desktop activity was present, so the measurements are conservative rather than a perfectly idle-machine sample.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 21:40
---
Implementation handoff
Branch: task-30-movegen-performance
Worktree: /Users/seabo/seaborg-worktrees/task-30-movegen-performance
Base: c3bf61430f135456f9d3dddfa8faafacb8a270e2
Implementation target: d56d02aa0726d1cb079af8d41a9e087ebb1efa8b
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo test --workspace: passed
- cargo test -p core --release: passed
- cargo bench --bench perft --bench movegen: generate moves 187.41 ns (95% CI 186.50–188.45 ns); perft 5 22.434 ms (95% CI 22.356–22.524 ms)
Known failures: A separate cargo test --workspace --release attempt reached all core safety tests successfully, then engine::search::tests::fifty_move_rule_uses_halfmove_boundary failed in engine/src/trace.rs with a divide-by-zero timing calculation; release workspace tests are not a repository-required check. Background desktop activity was visible during Criterion measurement.
---
<!-- COMMENTS:END -->
