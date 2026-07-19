---
id: TASK-55
title: Restore or deliberately remove mate-distance pruning in search
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-18 23:42'
updated_date: '2026-07-19 12:26'
labels:
  - engine
  - search
dependencies: []
priority: medium
type: bug
ordinal: 54000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-54 replaced the mate-distance pruning bounds in `Search::search` Step 2 with a clamp to `Score::mate(0)`..`Score::mate(1)`. That fixed a real defect: the previous bounds used the root-relative `draft`, which is wrong in an engine whose mate scores are position-relative. But the replacement no longer prunes anything.

`Score::mate(0)` (-20_100) is the minimum representable non-infinite score and `Score::mate(1)` (20_099) is the maximum, so the clamp is a no-op for every real score and only maps `INF_N`/`INF_P` inward. The `if alpha >= beta { return alpha }` early return is now reachable only in a degenerate way: a PV parent whose alpha is still `mate(0)` passes `Score(20_100)` as the child's alpha, which the child clamps back below its beta. Instrumenting `cargo test -p engine` on the merged result showed this return firing 1,544 times, every time with `alpha == Score(20_100)`, and never as a genuine mate-distance cutoff.

This is safe and `cargo bench --bench search` showed no measurable cost at startpos depth 7, where mate scores do not arise. The concern is that a standard search optimisation was silently dropped in positions where it does matter, and the comment above the block still describes pruning that no longer happens, which will mislead the next reader.

Decide deliberately between reinstating correct position-relative mate-distance pruning (which needs the node's ply-from-root, not `draft`) and removing the block outright with an honest comment. Do not leave it as-is.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 It is decided and recorded whether mate-distance pruning is reinstated or removed, with the reasoning stated in the code or the task
- [x] #2 If reinstated, the bounds are correct for position-relative mate scores and a test demonstrates a cutoff that the current clamp does not produce
- [x] #3 If removed, the dead clamp and its misleading 'Mate distance pruning' comment are gone and any still-needed INF clamping is stated as such
- [x] #4 Search behaviour on mate-rich positions is unchanged or improved, evidenced by the existing mate regression tests plus a debug self-play run over suites/wac.epd with no panic or hang
- [x] #5 cargo bench --bench search shows no repeatable regression against the pre-change commit on the same machine
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Deliberately remove mate-distance pruning because position-relative mate scores have no root-distance bound to tighten; retain only node-score window normalization required for exact child-bound and infinity inputs.
2. Rewrite search, quiescence, and Score documentation so the clamp and collapsed-window return are described solely as bound sanitation, not mate-distance pruning.
3. Retain focused regression coverage proving out-of-band child windows return valid in-band scores, then run mate-rich regressions and the debug wac.epd self-play test.
4. Compare cargo bench --bench search against the base commit, run all repository-required checks, commit the implementation, and record the immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Deliberately removed mate-distance pruning as a claimed optimisation: with position-relative mate scores, ply-from-root cannot tighten a node's attainable mate range, so reinstating the old root-relative bounds would be unsound. Retained the clamp and collapsed-window return solely as required node-score/INF/child-bound sanitation from TASK-56; production behavior is unchanged.

Verification evidence: focused out-of-band search and quiescence tests passed; gives_correct_answers passed; the ignored debug wac_root_scores_format_without_panicking sweep passed all 900 searches in 315.76s. Criterion target/base comparison at startpos depth 7 was target [40.457, 40.539, 40.639] us versus base [40.594, 40.663, 40.745] us; no-deadline target [40.314, 41.673, 43.458] us versus base [40.154, 40.227, 40.310] us, reported as no performance change. Required fmt, strict Clippy, and workspace tests all passed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 03:53
---
Implementation handoff
Branch: task-55-mate-distance-pruning
Worktree: /Users/seabo/seaborg-worktrees/task-55-mate-distance-pruning
Base: 79d82f018eb0b838cd9839e9d41d0aa0b7a2fd48
Implementation target: 13af47e7aa653810fae3d4556854f76cc07dc29c
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (43 core, 205 engine, 5 build-metadata, 1 doc; 0 failed)
- cargo test -p engine -- --ignored wac_root_scores_format_without_panicking --nocapture: passed (900 searches, 315.76s)
- cargo bench --bench search (target/base): no repeatable regression
Known failures: none
---

author: @codex
created: 2026-07-19 12:26
---
Review attempt: 1
Reviewed branch: task-55-mate-distance-pruning
Reviewed implementation: 13af47e7aa653810fae3d4556854f76cc07dc29c
Base: 79d82f018eb0b838cd9839e9d41d0aa0b7a2fd48
Verdict: approved

The decision is deliberate and technically consistent with the engine's position-relative mate encoding: root ply cannot tighten a descendant's attainable range. The former Step 2 block is no longer described as mate-distance pruning. Its retained clamp and collapsed-window return are documented solely as required node-score/INF/exact-child-bound sanitation from TASK-56, including why the degenerate early return remains necessary.

Acceptance evidence:
- AC#1/#2/#3: base-to-target inspection shows the production diff is comment/documentation-only in engine/src/search.rs and engine/src/score.rs. It records removal rather than reinstatement, removes the misleading Step 2 claim, and states why the retained normalization is required. AC#2 is not applicable because pruning was not reinstated.
- AC#4: cargo test --workspace passed, including gives_correct_answers, child_mate_windows_preserve_distance_parity, both out-of-band window regressions, and all existing mate tests. cargo test -p engine -- --ignored wac_root_scores_format_without_panicking --nocapture passed all 900 searches in 329.57s with debug assertions active.
- AC#5: executable behavior is unchanged because the implementation target modifies comments/task metadata only. The implementation's same-machine Criterion comparison reports target/base search intervals [40.457, 40.539, 40.639]/[40.594, 40.663, 40.745] us and no-deadline [40.314, 41.673, 43.458]/[40.154, 40.227, 40.310] us, with no reported performance change. An independent rerun was intentionally not used because an unrelated concurrent four-engine strength match made the host non-idle.

Verification:
- cargo fmt --check: passed
- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (43 core, 205 engine, 5 build-metadata, 1 doc; 0 failed)
- cargo test -p engine -- --ignored wac_root_scores_format_without_panicking --nocapture: passed (900 searches, 329.57s)
- git diff --check 79d82f0..13af47e: passed

Scope and immutability: 13af47e descends from the recorded base; the sole later commit c0d05d9 changes only TASK-55 handoff metadata. No implementation file changed after the target, no #[allow] was added, and the worktree was clean at review start.

Approved implementation SHA: 13af47e7aa653810fae3d4556854f76cc07dc29c
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Deliberately removed the obsolete mate-distance-pruning claim while retaining the required node-score window normalization introduced by TASK-56. The immutable implementation changes documentation only, explicitly explains the position-relative reasoning and sanitation behavior, and was verified by clean strict Clippy, the full workspace suite, focused mate/window regressions, the 900-search WAC debug sweep, and the recorded same-machine base/target search benchmark with no repeatable regression.
<!-- SECTION:FINAL_SUMMARY:END -->
