---
id: TASK-30
title: Restore move-generation performance without weakening domain safety
status: To Do
assignee: []
created_date: '2026-07-17 20:57'
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
