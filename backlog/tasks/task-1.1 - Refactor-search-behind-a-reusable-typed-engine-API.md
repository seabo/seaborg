---
id: TASK-1.1
title: Refactor search behind a reusable typed engine API
status: Changes Requested
assignee:
  - '@codex'
created_date: '2026-07-17 15:39'
updated_date: '2026-07-17 16:46'
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
- [ ] #1 Callers can start a search from a Position with a depth, time, or infinite limit and receive a typed final outcome
- [ ] #2 Iterative-deepening progress, score, nodes, NPS, principal variation, and current-move information are available as typed events rather than being printed by Search
- [ ] #3 A running search can be cancelled and reports an outcome that distinguishes completion from cancellation
- [ ] #4 UCI mode formats the typed events into its existing `info` and `bestmove` output without a behavior regression
- [ ] #5 Tests cover completed search, cancellation, event delivery, and UCI output formatting
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Introduce a public typed search lifecycle API with SearchLimit, SearchEvent, SearchResult/SearchOutcome, a cancellation token, and an asynchronous SearchHandle backed by the shared transposition table.
2. Refactor Search reporting to emit typed progress/current-move events through a channel and return structured results, with no protocol printing in the search layer.
3. Adapt the UCI driver to own/cancel active search handles and format typed events/outcomes into the existing info/bestmove text.
4. Update internal callers and add focused tests for completion, cancellation, event contents, and UCI formatting.
5. Run formatting and the full Rust workspace test suite, then record evidence and finalize the task.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the reusable typed search lifecycle in engine::search: SearchEngine/SearchHandle, SearchLimit (depth/time/infinite), CancellationToken, SearchEvent progress/current-move payloads, and Completed/Cancelled SearchOutcome. Search no longer prints protocol output. Adapted the UCI driver to render typed events and outcomes through engine::info, retain active search handles, and cancel/join them safely. Added lifecycle and exact UCI-formatting tests, plus fixed cancellation unwind ordering so timed stops do not trip the move-count assertion.

Verification: cargo fmt --check passes. All 7 task-specific typed lifecycle/UCI formatting tests pass. Manual `--uci` smoke test with `go depth 2` emitted two info lines and `bestmove a2a3`. `cargo test --workspace` ran but the engine suite retains two pre-existing failures: search::tests::gives_correct_answers (alpha < beta debug assertion) and tt::tests::gen_bound (gen < 64 debug assertion). Both were reproduced against an untouched archive of HEAD; the task changes introduce no additional workspace failures.

Final focused verification now covers 8 tests (including direct typed current-move event delivery); all pass. git diff --check also passes.
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
<!-- COMMENTS:END -->
