---
id: TASK-37
title: Add regression coverage for the stdin-EOF / stop-abort bestmove path
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 01:21'
updated_date: '2026-07-18 23:13'
labels:
  - engine
  - search
  - uci
dependencies:
  - TASK-32
modified_files:
  - engine/src/engine.rs
priority: medium
type: bug
ordinal: 42000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
NARROWED BY TASK-34 REWORK (2026-07-18). The original defect — 'bestmove 0000' on stdin EOF while searching — NO LONGER REPRODUCES. It was fixed as a side effect of TASK-32 (merged at 8ceb480), whose Search::min_search_complete makes stopping() return false until the first full ply completes, suppressing BOTH the time deadline and the cancellation flag. Because EOF reaches the search through that same cancellation flag (engine/src/engine.rs:90 Input::Closed -> cancel -> finish_search), a completed legal root move is always recorded before any EOF-driven abort can take effect.

Re-verified on the TASK-34 branch against merged TASK-32 code (release build, commit d6c5679):
  printf 'uci/isready/go depth 25'                        -> bestmove a2a3   (was: 0000)
  printf 'uci/isready/position startpos/go depth 8'       -> bestmove a2a3   (was: 0000)
  printf 'uci/isready/position fen <Kiwipete>/go depth 20'-> bestmove e2a6   (legal)
  printf 'uci/isready/position startpos/go infinite'      -> bestmove a2a3
  printf 'uci/isready/position startpos/go depth 25/quit' -> bestmove a2a3
Past ply 1 the abort yields Cancelled(Some(result)) and the last completed iteration's move is returned: EOF after ~3s of 'go infinite' returned the depth-10 result (bestmove a2a3). Terminal positions still correctly emit 'bestmove 0000' (checkmate 7k/5QQ1/8/8/8/8/8/7K b, stalemate 7k/5Q2/6K1/8/8/8/8/8 b).

The fix-level guarantee is therefore already implemented and no engine behaviour change is required. What remains is the REGRESSION COVERAGE that TASK-34 AC #4 requires to be carried forward. TASK-32's unit tests cover the search-level abort paths (zero_time_limit_still_returns_a_legal_move, near_zero_time_budget_completes_the_guaranteed_ply, aborts_are_suppressed_only_until_the_first_ply_completes, immediate_cancellation_returns_an_explicit_optional_result), but nothing exercises the DRIVER-level EOF path (Input::Closed while a search is running) end to end, and nothing pins the terminal-position 'bestmove 0000' case. Without that, a future change to the driver's EOF handling or to min_search_complete could silently reintroduce the forfeit.

Scope: tests only. Do not re-implement the guarantee — that is TASK-32's, already merged. If implementing this reveals the guarantee is actually incomplete, stop and file a fix ticket rather than widening this one.

Relevant code: engine/src/engine.rs (Input::Closed handling at :90, stop_search/finish_search), engine/src/search.rs (min_search_complete, stopping() at :763), engine/src/info.rs (format_search_outcome).

Coordination: TASK-39 asks whether TASK-32's abort-suppressed window bounds UCI 'stop' responsiveness. That is a live open question about the same window this ticket tests; if TASK-39 changes the suppression semantics, these tests must be written to still hold (assert a LEGAL move is returned, not a specific depth or timing).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A driver-level regression test exercises the stdin-EOF path end to end (start a search, deliver Input::Closed / close stdin while it is running) in a non-terminal position and asserts the emitted bestmove is a legal move, never 'bestmove 0000'
- [ ] #2 A regression test pins the terminal-position behaviour: a checkmated or stalemated position still emits 'bestmove 0000' (e.g. 7k/5QQ1/8/8/8/8/8/7K b and 7k/5Q2/6K1/8/8/8/8/8 b)
- [ ] #3 Both tests fail if the guaranteed-minimum-search behaviour is removed (verified by temporarily reverting or stubbing Search::min_search_complete), so they genuinely pin the guarantee rather than passing incidentally
- [ ] #4 The tests assert only that a legal move is returned, not a specific depth, move, or timing, so they remain valid if TASK-39 changes the abort-suppression window
- [ ] #5 No engine behaviour change lands under this ticket: the diff is tests-only (plus any test-support plumbing), and the existing search and UCI test suites still pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extend the existing engine driver test harness in engine/src/engine.rs with helpers that run the real reader/driver/search path through EOF and extract the single emitted bestmove.
2. Add a non-terminal infinite-search EOF regression that validates the emitted UCI move against the searched Position, without asserting move choice, depth, or timing.
3. Add a driver-level terminal-position regression for checkmate and stalemate, retaining a non-terminal control so the test also detects removal of the guaranteed-minimum-search behavior while asserting terminal positions emit bestmove 0000.
4. Prove test sensitivity by temporarily disabling the min_search_complete guard, restore production code, then run focused tests and all repository-required checks.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added driver-level stdin EOF regression coverage in engine/src/engine.rs. The tests exercise the real reader, Input::Closed handling, search cancellation, outcome formatting, and stdout path. Non-terminal outputs are parsed and applied to the original Position to prove legality; checkmate and stalemate pin bestmove 0000. A non-terminal control in each test makes both sensitive to the guaranteed-minimum-search behavior.

Mutation evidence: temporarily changed Search::stopping so min_search_complete no longer suppressed cancellation, then ran cargo test -p engine engine::tests::stdin_eof_. Both tests failed because the non-terminal EOF path emitted bestmove 0000. Restored search.rs unchanged before verification.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 12:03
---
Narrowed by the TASK-34 rework (review attempt 1 / merge attempt 1 ejection).

The original defect no longer reproduces: TASK-32 (merged 8ceb480) fixed it as a side effect. Its Search::min_search_complete suppresses the cancellation flag until ply 1 completes, and stdin EOF reaches the search through exactly that flag, so a legal root move is always recorded first. Re-verified across five EOF variants plus a post-ply-1 abort on the merged code; evidence is in this ticket's description and in doc-2.

This ticket therefore no longer specs an engine fix. It retains only the regression coverage that TASK-34 AC #4 requires to be carried forward, because TASK-32's unit tests cover the search-level abort paths but nothing exercises the driver-level EOF path (Input::Closed during a live search) end to end and nothing pins the terminal-position 'bestmove 0000' case. Priority dropped high -> medium accordingly: this is defence against regression, not a live defect.

It was NOT retired outright precisely so that requirement does not become homeless. Ordinal moved 40000 -> 42000 to clear the collision with TASK-38/TASK-39 filed on master.
---

author: @codex
created: 2026-07-18 23:13
---
Implementation handoff
Branch: task-37-stdin-eof-regression
Worktree: /Users/seabo/seaborg-worktrees/task-37-stdin-eof-regression
Base: ebf428924df7afef6616ad179b6c186d0faa4b6b
Implementation target: 48db98524f7eb2b7f585327d50c99b2b31845f58
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (35 core + 163 engine + 5 integration + 1 doctest; 0 failed, 1 ignored)
- cargo test -p engine engine::tests::stdin_eof_ repeated 20 times: passed (40/40 test executions)
- Mutation check with min_search_complete suppression bypassed: both new tests failed on non-terminal bestmove 0000; production guard restored before required checks
Known failures: none
---
<!-- COMMENTS:END -->
