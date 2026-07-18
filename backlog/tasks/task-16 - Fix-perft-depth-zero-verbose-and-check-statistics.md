---
id: TASK-16
title: 'Fix perft depth-zero, verbose, and check statistics'
status: Done
assignee:
  - '@george'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-18 00:52'
labels:
  - perft
  - cli
dependencies: []
references:
  - engine/src/perft.rs
  - src/perft.rs
priority: medium
type: bug
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Perft depth zero underflows into recursion, the CLI verbose flag is ignored, and check statistics count only double checks. Make the CLI and library edge cases consistent and accurate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Perft at depth zero returns exactly one node without recursion or panic
- [x] #2 The CLI verbose flag enables the documented detailed counters and timing output
- [x] #3 The check counter includes every checking leaf while double checks are not substituted for all checks
- [x] #4 Divide rejects or handles depth zero consistently with normal perft
- [x] #5 Tests cover depth zero and known detailed perft statistics
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Fix depth-zero in Perft::perft_inner: return exactly one node before any movegen/recursion (avoids usize underflow on depth-1).
2. Wire the CLI --verbose flag (src/perft.rs) to collect_detailed_data so documented detailed counters + timing print; keep divide behaviour consistent.
3. Fix check counter in handle_leaf: count every checking leaf via Position::in_check() instead of in_double_check(); keep checkmate counter.
4. Make Perft::divide handle depth zero consistently with perft (return one node, no panic) instead of assert.
5. Add tests: depth-zero returns 1 node for perft and divide; detailed statistics (nodes/captures/ep/castles/promotions/checks/checkmates) against known chessprogramming.org values for start position and Kiwipete.
6. Run cargo fmt --check and cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation summary:
- engine/src/perft.rs: perft_inner now returns immediately at depth 0 (one node), removing the usize underflow (0 - 1) that previously drove unbounded recursion / panic.
- engine/src/perft.rs: Perft::divide handles depth 0 the same way (one node, no per-move divide, no panic) replacing the assert!(depth >= 1).
- engine/src/perft.rs: handle_leaf counts checking leaves with Position::in_check() (single or double check) instead of in_double_check(); checkmates remain a subset, matching chessprogramming.org tables.
- src/perft.rs: CLI --verbose now drives collect_detailed_data for both perft and divide, so documented detailed counters (captures/ep/castles/promotions) print alongside the existing timing output.
- Tests added: perft_depth_zero_counts_single_node, divide_depth_zero_matches_perft, perft_detailed_statistics (start depth 3, Kiwipete depth 2, cpwiki position 4 depth 3 — covers captures, ep, castles, promotions, checks, checkmates against published references).
Verification: cargo fmt --check clean; cargo test --workspace all green; manual CLI checks for verbose output and depth-0 (perft and divide return 1 node without hang).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-17 23:35
---
Implementation handoff
Branch: task-16-perft-fixes
Worktree: /Users/seabo/seaborg-worktrees/task-16-perft-fixes
Base: 299f5ec549b6b6983ff0b0f08b4460bfb1853335
Implementation target: 58d5cc957f0c3cca762ff24b93a1013a237c7b59
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo test --workspace: pass (core 35, engine 63/1 ignored, build_metadata 5, doctests ok)
- manual: cargo run -- perft -n 3 --verbose prints detailed counters + timing; perft -n 0 and perft -n 0 --divide each return 1 node with no recursion
Known failures: none
---

author: @george
created: 2026-07-18 00:45
---
Review attempt: 1
Reviewed branch: task-16-perft-fixes
Reviewed implementation: 58d5cc957f0c3cca762ff24b93a1013a237c7b59
Base: 299f5ec549b6b6983ff0b0f08b4460bfb1853335
Verdict: approved

All acceptance criteria proven against the immutable base->target diff (only engine/src/perft.rs and src/perft.rs changed):
- AC#1: perft_inner returns one node at depth 0 with no recursion/panic. Evidence: test perft_depth_zero_counts_single_node; CLI 'perft -n 0' -> Nodes: 1.
- AC#2: --verbose enables the documented detailed counters + timing. Evidence: CLI 'perft -n 3 --verbose' prints Captures/Ep/Castles/Promotions + Time; 'perft -n 3' prints nodes only.
- AC#3: handle_leaf counts every checking leaf via in_check() (single or double), checkmates a subset. Evidence: test perft_detailed_statistics asserts checks 12/3/38 and checkmates 0/0/22 vs chessprogramming.org.
- AC#4: divide returns one node at depth 0. Evidence: test divide_depth_zero_matches_perft; CLI 'perft -n 0 --divide' -> Nodes: 1.
- AC#5: new tests cover depth zero and known detailed statistics.

Verification:
- cargo fmt --check: pass
- cargo test --workspace: pass (core 35, engine 63/1 ignored, build_metadata 5, doctests ok)
- manual CLI: verbose/non-verbose and depth-0 perft+divide as above

Hot-path benchmarks: not run. Diff touches only perft.rs; movegen untouched. The depth-0 early return is unreachable during normal recursion (leaves handled at depth==1) and the check-counter reorder is inside the options.checks branch, so the default perft recursion hot loop is byte-equivalent to base.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed perft depth-zero underflow, wired the CLI --verbose flag, and corrected check-leaf counting. engine/src/perft.rs: perft_inner and divide return exactly one node at depth 0 (removing the usize depth-1 underflow); handle_leaf counts every checking leaf via Position::in_check() (single or double), with checkmates tracked as a subset. src/perft.rs: --verbose now drives collect_detailed_data for perft and divide. Verified on target 58d5cc9: cargo fmt --check clean; cargo test --workspace green (core 35, engine 63/1 ignored, build_metadata 5, doctests ok); new tests perft_depth_zero_counts_single_node, divide_depth_zero_matches_perft, perft_detailed_statistics (start d3, Kiwipete d2, cpwiki-pos4 d3 vs published captures/ep/castles/promotions/checks/checkmates) pass; manual CLI: perft -n 0 and perft -n 0 --divide each return 1 node without recursion, perft -n 3 --verbose prints detailed counters + timing while non-verbose prints nodes only.
<!-- SECTION:FINAL_SUMMARY:END -->
