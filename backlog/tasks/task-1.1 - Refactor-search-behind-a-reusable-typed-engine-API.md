---
id: TASK-1.1
title: Refactor search behind a reusable typed engine API
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 15:39'
updated_date: '2026-07-17 18:17'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
modified_files:
  - engine/src/search.rs
  - engine/src/info.rs
  - engine/src/engine.rs
parent_task_id: TASK-1
type: task
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decouple search execution and reporting from the current stdin/stdout UCI driver so browser and UCI integrations can consume the same typed search lifecycle. This is the prerequisite for all UI runtime work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Callers can start a search from a Position with a depth, time, or infinite limit and receive a typed final outcome
- [x] #2 Iterative-deepening progress, score, nodes, NPS, principal variation, and current-move information are available as typed events rather than being printed by Search
- [x] #3 A running search can be cancelled and reports an outcome that distinguishes completion from cancellation
- [x] #4 UCI mode formats the typed events into its existing `info` and `bestmove` output without a behavior regression
- [x] #5 Tests cover completed search, cancellation, event delivery, and UCI output formatting
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Model a search with no completed iteration or no legal move explicitly, while preserving terminal scores and valid completed results.
2. Emit progress only after a fully completed iterative-deepening iteration and add regression tests for immediate cancellation, zero time, terminal positions, and cancellation event consistency.
3. Update UCI outcome formatting so absent best moves use the protocol null move `0000`, then run focused tests and required workspace checks.
4. Record each REV-1-* resolution, commit the immutable implementation target, and return the task to In Review with verification evidence.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the reusable typed search lifecycle in engine::search: SearchEngine/SearchHandle, SearchLimit (depth/time/infinite), CancellationToken, SearchEvent progress/current-move payloads, and Completed/Cancelled SearchOutcome. Search no longer prints protocol output. Adapted the UCI driver to render typed events and outcomes through engine::info, retain active search handles, and cancel/join them safely. Added lifecycle and exact UCI-formatting tests, plus fixed cancellation unwind ordering so timed stops do not trip the move-count assertion.

Verification: cargo fmt --check passes. All 7 task-specific typed lifecycle/UCI formatting tests pass. Manual `--uci` smoke test with `go depth 2` emitted two info lines and `bestmove a2a3`. `cargo test --workspace` ran but the engine suite retains two pre-existing failures: search::tests::gives_correct_answers (alpha < beta debug assertion) and tt::tests::gen_bound (gen < 64 debug assertion). Both were reproduced against an untouched archive of HEAD; the task changes introduce no additional workspace failures.

Final focused verification now covers 8 tests (including direct typed current-move event delivery); all pass. git diff --check also passes.

Resolved REV-1-01: SearchOutcome now carries an optional completed iteration and SearchResult carries an optional best move, eliminating sentinel score/move results. Zero-time and immediate-cancellation regressions cover the boundary; UCI renders absent moves as `bestmove 0000`.

Resolved REV-1-02: Terminal searches retain their typed score/depth while representing the absence of a legal best move explicitly, without probing an assumed transposition-table entry. The reviewer FEN is covered by a regression test.

Resolved REV-1-03: Progress events are emitted only inside the fully completed iteration branch. Cancellation coverage asserts emitted PV lengths remain consistent with their reported depth.

Rework verification: cargo fmt --check passed; focused typed lifecycle, immediate cancellation, zero-time, terminal-position, and UCI formatting tests passed; git diff --check passed. cargo test --workspace --no-fail-fast completed with 21 engine tests passing, 1 ignored, and the known baseline failure tt::tests::gen_bound (assertion gen < 64).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex-reviewer
created: 2026-07-17 16:46
---
Review attempt: 1
Reviewed target: uncommitted TASK-1.1 working-tree diff over master@6e9502a6760e400bd6195b31365aeeb415d72673
Verdict: changes_requested

REV-1-01 [P1] Searches can return a fabricated unusable result
Location: engine/src/search.rs iterative_deepening initialization and result construction
Reproduction: immediate go infinite + stop, or go movetime 0, emits bestmove Null from score INF_N / depth 0 sentinel state.
Expected: represent absence of a completed result explicitly or choose a valid fallback move; UCI must not emit Null as a move.

REV-1-02 [P2] Valid terminal positions panic instead of returning a typed outcome
Location: engine/src/search.rs empty-PV transposition-table fallback
Reproduction: search depth 1 from FEN 7k/6Q1/6K1/8/8/8/8/8 b - - 0 1.
Expected: return the terminal score and an explicit no-best-move result without panicking.

REV-1-03 [P2] Cancellation emits internally inconsistent progress
Location: engine/src/search.rs progress emission after iterative-deepening search
Reproduction: cancel during depth 11; an info depth 10 event was emitted with an 11-move partial PV.
Expected: emit completed-iteration progress only inside the successful iteration branch, or model partial progress explicitly.

Verification: cargo fmt --check passed; all 8 TASK-1.1-focused tests passed; git diff --check passed. cargo test --workspace retained the documented pre-existing failures search::tests::gives_correct_answers and tt::tests::gen_bound.

Required handoff: resolve each REV-1-* finding on this task, record the resolving commit and tests, then return the task to In Review for a fresh full review.
---

author: @codex
created: 2026-07-17 18:06
---
Rework started for REV-1-01, REV-1-02, and REV-1-03 on the existing task branch and worktree.
---

author: @codex
created: 2026-07-17 18:11
---
Implementation handoff
Branch: task-1.1-typed-engine-api
Worktree: /Users/seabo/seaborg-worktrees/task-1.1-typed-engine-api
Base: 4dd57abfdc18b0f70c4d6030e4ced0ef8c19b3ab
Implementation target: f3a521d
Resolved findings: REV-1-01, REV-1-02, REV-1-03
Verification:
- cargo fmt --check: passed
- focused typed lifecycle, immediate cancellation, zero-time, terminal-position, and UCI formatting tests: passed
- git diff --check: passed
- cargo test --workspace --no-fail-fast: 21 engine tests passed, 1 ignored; one known baseline failure
Known failures: tt::tests::gen_bound asserts gen < 64, previously documented as pre-existing.
---

author: @codex-reviewer
created: 2026-07-17 18:15
---
Review attempt: 2
Reviewed branch: task-1.1-typed-engine-api
Reviewed implementation: f3a521d
Verdict: approved

Resolved findings verified: REV-1-01, REV-1-02, REV-1-03.

Verification:
- cargo fmt --check: passed
- focused typed API, cancellation, terminal-position, current-move delivery, and UCI formatting tests: passed
- git diff --check 4dd57ab..f3a521d: passed
- cargo test --workspace --no-fail-fast: 22 engine tests passed, 1 ignored; tt::tests::gen_bound failed
- baseline cargo test -p engine tt::tests::gen_bound -- --exact on master: same gen < 64 assertion failed

Approval applies to immutable implementation f3a521d; commits after it contain task metadata only.
---

author: @codex
created: 2026-07-17 18:17
---
Merged approved task branch task-1.1-typed-engine-api at 88e138e into master via merge commit 12d0324. Approved implementation target: f3a521d.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Introduced a reusable typed asynchronous search API with depth, time, and infinite limits; structured progress/current-move events; explicit completion and cancellation outcomes; and UCI adapter formatting. Verified implementation f3a521d with cargo fmt --check, focused lifecycle/terminal/cancellation/UCI tests, git diff --check, and cargo test --workspace --no-fail-fast (22 engine tests passed, 1 ignored; baseline tt::tests::gen_bound failure reproduced on master).
<!-- SECTION:FINAL_SUMMARY:END -->
