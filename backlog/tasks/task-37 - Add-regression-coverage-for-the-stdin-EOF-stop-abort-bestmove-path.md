---
id: TASK-37
title: Add regression coverage for the stdin-EOF / stop-abort bestmove path
status: Done
assignee:
  - '@codex'
created_date: '2026-07-18 01:21'
updated_date: '2026-07-18 23:28'
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
- [x] #1 A driver-level regression test exercises the stdin-EOF path end to end (start a search, deliver Input::Closed / close stdin while it is running) in a non-terminal position and asserts the emitted bestmove is a legal move, never 'bestmove 0000'
- [x] #2 A regression test pins the terminal-position behaviour: a checkmated or stalemated position still emits 'bestmove 0000' (e.g. 7k/5QQ1/8/8/8/8/8/7K b and 7k/5Q2/6K1/8/8/8/8/8 b)
- [x] #3 Both tests fail if the guaranteed-minimum-search behaviour is removed (verified by temporarily reverting or stubbing Search::min_search_complete), so they genuinely pin the guarantee rather than passing incidentally
- [x] #4 The tests assert only that a legal move is returned, not a specific depth, move, or timing, so they remain valid if TASK-39 changes the abort-suppression window
- [x] #5 No engine behaviour change lands under this ticket: the diff is tests-only (plus any test-support plumbing), and the existing search and UCI test suites still pass
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

author: @codex
created: 2026-07-18 23:24
---
Review verdict: APPROVED (review attempt 1)

Implementation target: 48db98524f7eb2b7f585327d50c99b2b31845f58
Base: ebf428924df7afef6616ad179b6c186d0faa4b6b
Branch: task-37-stdin-eof-regression
Worktree: /Users/seabo/seaborg-worktrees/task-37-stdin-eof-regression

Target immutability: 48db985 confirmed an ancestor of tip 6e94081; the only later commit (6e94081) touches the task file alone. Worktree clean. Reviewed the full base-to-target diff, not just the latest commit.

Scope: base..target changes engine/src/engine.rs (+45) and the task file. Every engine.rs hunk lands inside 'mod tests' (starts at line 299), so the diff is tests-only. No production code, no new #[allow], no unrelated edits. Benchmarks not run: the diff adds only #[cfg(test)] code and cannot touch movegen or search hot paths.

Acceptance criteria:
#1 PROVEN - stdin_eof_during_search_emits_a_legal_bestmove runs the real run() driver over 'go infinite\n' bytes, so the reader hits EOF while the search is genuinely live ('go infinite' can never self-terminate). Legality is real, not textual: core::Position::make_uci_move (core/src/position/mod.rs:547) resolves the string against generate::<_, All, Legal> and returns None for anything illegal.
#2 PROVEN - stdin_eof_emits_null_bestmove_only_for_terminal_positions asserts 'bestmove 0000' for both the checkmate and stalemate FENs named in the criterion. The tests also terminate, which incidentally proves the terminal-position EOF path does not hang.
#3 PROVEN INDEPENDENTLY - I did not rely on the handoff's mutation evidence. In a separate scratch worktree at 48db985 I removed the 'if !self.min_search_complete { return false; }' guard from Search::stopping (engine/src/search.rs:821) and ran 'cargo test -p engine engine::tests::stdin_eof_': both tests FAILED with 'non-terminal EOF returned a null move' (left: "0000"). The guard was reverted and the scratch worktree removed. The non-terminal control inside the terminal test is what makes the second test sensitive too.
#4 PROVEN - the only assertions are bestmove != "0000", make_uci_move(...).is_some(), exactly one bestmove emitted, and empty post-banner stderr. No depth, move-choice, or timing expectation, so a TASK-39 change to the suppression window cannot invalidate them.
#5 PROVEN - tests-only diff as above; full workspace suite green.

Verification commands run by the reviewer on 48db985:
- cargo fmt --check: passed
- CARGO_TARGET_DIR=/tmp/t37-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings: passed, 0 warnings. Run with a clean target dir because the first (cached) run finished in 1.03s and could not be treated as load-bearing evidence.
- cargo test --workspace: passed (core 35, engine 163 with 1 ignored, seaborg 0, build_metadata 5, doctests 1; 0 failed)
- 25 consecutive runs of 'cargo test -p engine engine::tests::stdin_eof_': 50/50 test executions passed, no flakiness in the EOF-versus-ply-1 race
- Mutation sensitivity check described under #3

Non-blocking observation, recorded for context and deliberately not raised as a finding: assert_eof_returns_legal_move takes a Position and a position_command independently and does not check that they agree, so a future caller passing a mismatched pair would validate legality against the wrong board. Both current call sites pass startpos consistently, so nothing is wrong today.

Verdict: every acceptance criterion is proven by objective evidence, no blocking findings. Moving to Ready to Merge with 48db98524f7eb2b7f585327d50c99b2b31845f58 as the approved code target.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added driver-level regression coverage for the stdin-EOF bestmove path in engine/src/engine.rs (test module only; no production code changed). stdin_eof_during_search_emits_a_legal_bestmove drives the real reader/driver/search stack with 'go infinite' followed by EOF from startpos and validates the emitted bestmove against core::Position::make_uci_move, which resolves the move against legal move generation. stdin_eof_emits_null_bestmove_only_for_terminal_positions pins 'bestmove 0000' for the checkmate (7k/5QQ1/8/8/8/8/8/7K b) and stalemate (7k/5Q2/6K1/8/8/8/8/8 b) FENs while retaining a non-terminal control so it stays sensitive to the guarantee. Helper bestmove_from also asserts exactly one bestmove is emitted. Assertions are legality-only, with no depth, move-choice, or timing expectations, so they survive a change to TASK-39's abort-suppression window. Verified independently at 48db98524f7eb2b7f585327d50c99b2b31845f58: cargo fmt --check, clean-CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings (0 warnings), cargo test --workspace (35 core + 163 engine + 5 integration + 1 doctest; 0 failed), 25 consecutive repeat runs of engine::tests::stdin_eof_ (50/50 executions passed), and a reviewer-run mutation in a scratch worktree removing the min_search_complete guard from Search::stopping, under which both new tests failed on non-terminal 'bestmove 0000'.
<!-- SECTION:FINAL_SUMMARY:END -->
