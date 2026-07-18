---
id: TASK-32
title: Engine returns illegal null move and forfeits at fast time controls
status: Done
assignee:
  - '@georgeseabridge'
created_date: '2026-07-18 00:09'
updated_date: '2026-07-18 11:56'
labels:
  - engine
  - search
  - time
dependencies: []
priority: high
type: bug
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Seaborg's time management does not guarantee a legal move within a small time budget. On master, TimeControl::to_move_time (engine/src/time.rs) saturates the per-move allocation to 0ms at fast time controls (e.g. UCI 'go' derived from tc=2+0.05). The search (engine/src/search.rs) initializes best_move = Move::null() and, when the budget aborts it before even a depth-1 iteration completes, returns that null move, which UCI emits as 'bestmove 0000'. Tournament runners (FastChess, cutechess-cli) reject 0000 as an illegal move and forfeit the game, so seaborg loses every game at standard blitz time controls. At more generous controls (tc=10+0.1) it plays legally but slowly; a fixed-depth limit (go depth N) always plays legally.

Discovered while validating the TASK-27 strength-regression tooling against a real FastChess build: seaborg-vs-seaborg at tc=2+0.05 produces 'Illegal move 0000 played' forfeits, making authoritative timed strength testing of seaborg impractical until this is fixed. This does not block delivering the TASK-27 tool itself (which is runner-agnostic and can drive a fixed-depth smoke match), but it does block meaningful timed self-play of seaborg. Note the historical stale-base variant of this bug (u32 underflow producing a huge budget, i.e. loss on time) was addressed by TASK-7; this ticket covers the remaining zero/near-zero-budget null-move defect.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 For any go command, the engine returns a legal move whenever a legal move exists, including when the computed time budget is zero or near-zero (never 'bestmove 0000' in a non-terminal position)
- [x] #2 A guaranteed-minimum search completes at least one full ply / one legal root move before any time-based abort can take effect
- [x] #3 The search honors the allotted clock: self-play at fast time controls (e.g. 2+0.05 and 10+0.1) produces no illegal-move forfeits and no losses on time attributable to overrun
- [x] #4 Behavior is validated with a UCI tournament runner (FastChess or cutechess-cli) playing seaborg self-play at a fast time control with zero illegal moves and zero time forfeits
- [x] #5 Unit tests cover the zero/near-zero budget path returning a legal move and the time-abort honoring the budget
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Root cause: with a zero/near-zero time budget, Search::stopping() reports true immediately (stop_time already elapsed), so iterative_deepening breaks before recording any result; the outcome carries best_move=None and info::format_search_outcome emits 'bestmove 0000', which runners reject.
2. Fix in engine/src/search.rs: add a guaranteed-minimum-search guard. Suppress the time-based deadline in stopping() until the first full ply (depth-1 root iteration) has completed. After depth 1 records a result, arm the time abort so deeper iterations honor the clock. This guarantees a completed legal root move whenever one exists, without overrunning the budget beyond one (fast) ply.
3. Keep the cancellation (user 'stop') path and Depth/Infinite limits unchanged; only the time deadline is gated. Leave time.rs allocation as-is (saturate-to-zero is now safe).
4. Update the existing zero_time_limit test (which asserted the buggy Completed(None)) to assert a legal move is returned; add near-zero-budget and time-abort-honoring unit tests.
5. Validate with FastChess seaborg self-play at a fast time control (e.g. 2+0.05 and 10+0.1): zero illegal moves, zero time forfeits.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation complete.

Root cause: iterative_deepening() broke out of its loop as soon as Search::stopping() reported true. With a zero/near-zero time budget the stop_time is already elapsed, so the loop broke before recording any result; the SearchOutcome carried best_move=None and info::format_search_outcome emitted 'bestmove 0000', which runners reject as an illegal move -> forfeit.

Fix (engine/src/search.rs): added Search::min_search_complete. stopping() returns false until the first full ply completes, suppressing BOTH the cancellation flag and the time deadline; it is armed (set true) after the first iterative-deepening iteration, so deeper iterations honor the clock. The first ply is finite, so this cannot hang. Net effect: a completed, searched, legal root move is always available whenever one exists, at any budget (including 0), and even on an immediate stop.

Decision: suppressing the cancellation flag (not only the time deadline) during ply 1 was necessary because an immediate 'stop'/'quit' arriving during ply 1 otherwise still produced 'bestmove 0000' (reproduced manually). This makes AC #1 hold absolutely. The one pre-existing test that used the cancel flag to force an immediate abort (quiescence_abort_with_legal_evasions_is_not_checkmate) was updated to set min_search_complete=true first: at runtime an in-flight quiesce_evasions can only be aborted after ply 1, so the test now reflects the real precondition rather than masking a regression.

Scope note: FastChess logs 'Warning; Illegal PV move' on mate scores. This is a pre-existing artifact of PV/mate reporting (PVTable population and info.rs formatting are untouched by this change) and concerns only the info-line PV, not the played move: all games terminate as real checkmates/draws with no illegal-move or time adjudications. Out of scope for TASK-32.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 01:22
---
TASK-34 investigation (doc-2) found that the stdin-EOF null-move defect (then specced as TASK-37) shares this ticket's root cause: no guaranteed legal root move before an abort, yielding 'bestmove 0000'. Only the abort trigger differs (TASK-32: zero/near-zero time budget; TASK-37: stdin EOF/cancel). The shared 'always choose a legal move before any abort; return legal best-so-far' guarantee should be implemented once.

Resolved by this ticket's own implementation. Search::min_search_complete suppresses both the time deadline and the cancellation flag until ply 1 completes, and EOF reaches the search through that same cancellation flag (engine.rs:90 Input::Closed -> stop_search -> cancel()), so the EOF trigger is covered too. TASK-34 re-verified this against the merged code (release build d6c5679): five EOF variants all return legal moves where master d9a138c returned 'bestmove 0000', an abort after ply 1 returns the last completed iteration's move, and terminal positions still correctly emit 0000.

The predicted single shared guarantee therefore landed here, once, as intended. TASK-37 was narrowed to regression coverage only (driver-level EOF path and terminal-position case, tests only, no engine change) rather than retired, so that TASK-34 AC #4's requirement to carry forward regression coverage of the stop/abort and EOF paths is not dropped. No further fix-level coordination is required. See also TASK-39, which asks whether this suppressed window bounds UCI 'stop' responsiveness; any narrowing of the window must preserve this EOF guarantee.
---

author: @georgeseabridge
created: 2026-07-18 01:27
---
Implementation handoff
Branch: task-32-illegal-null-move
Worktree: /Users/seabo/seaborg-worktrees/task-32-illegal-null-move
Base: d9a138ccdeb36f39dd28fc7e19d460635ec6be29
Implementation target: f4a4643591d5349db80815fd8cec36cd947ee7f6
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --all --check: clean
- cargo clippy --workspace --lib --bins --tests: no errors (pre-existing warnings only)
- cargo test --workspace --lib --bins --tests: ok (35 + 71[1 ignored] + 5 passed, 0 failed)
- Manual UCI, zero budget + immediate quit (echo 'uci/isready/go wtime 2000 btime 2000 winc 50 binc 50/quit' | seaborg --uci): 'bestmove a2a3' (legal), no 'bestmove 0000'
- FastChess self-play 2+0.05, 40 games: all Termination=normal, 0 illegal-move forfeits, 0 time losses (20-20-0)
- FastChess self-play 10+0.1, 20 games: all Termination=normal, 0 illegal-move forfeits, 0 time losses (6-6-8 W/L/D)
Known failures: none from this change. Baseline (pre-existing, unrelated): 'benches/square.rs' fails to compile (E0423, Square tuple-struct private field) on master; excluded via --lib/--bins/--tests. FastChess 'Illegal PV move' warnings are a pre-existing PV-info-line artifact on mate scores (played moves are legal; games end normally).
---

author: @georgeseabridge
created: 2026-07-18 11:38
---
Review attempt: 1
Reviewed branch: task-32-illegal-null-move
Reviewed implementation: f4a4643591d5349db80815fd8cec36cd947ee7f6
Base: d9a138ccdeb36f39dd28fc7e19d460635ec6be29
Verdict: approved

Target immutability: f4a4643 descends from the recorded base; the only later commit (c0166d4) touches the task file alone.

Scope: base..target changes engine/src/search.rs (fix + tests), engine/src/game.rs (test only), and the task file. No unrelated work. The replaced game.rs test 'incomplete_search_outcomes_are_ignored' loses no coverage: 'stale_or_cancelled_search_outcomes_are_never_applied' still exercises the SearchOutcome::Completed(Some(_)) guard at game.rs:182.

Design review: run() is invoked exactly once per search (search.rs:151), so the guaranteed ply cannot be re-armed mid-search; min_search_complete is a plain field on a single-threaded Search, so there is no concurrency exposure; the first iteration is always d=1, so the suppressed window is bounded by one depth-1 search plus quiescence.

AC #1 - Independently reproduced the defect on the base commit: 'go wtime 2000 btime 2000 winc 50 binc 50' returns 'bestmove 0000'. On the target the same command returns 'bestmove a2a3'. Legal moves also returned for 'go movetime 0', 'go wtime 1 btime 1', and 'go infinite' + immediate 'stop', across startpos, Kiwipete (r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1 -> e2a6), a pawn endgame (8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1 -> b4f4), and a rook endgame (-> a1a8). A checkmated position still returns 0000, which the criterion explicitly scopes out ('in a non-terminal position') and which is conventional UCI behavior.

AC #2 - stopping() (search.rs:761) returns false until min_search_complete, which iterative_deepening sets only after an iteration completes (search.rs:466). Unit test aborts_are_suppressed_only_until_the_first_ply_completes asserts both halves with the flag set and the deadline already elapsed. Every move in the tournament PGNs reports depth >= 1.

AC #3 / #4 - FastChess self-play on the target build: 40 games at tc=2+0.05 (20-20-0) and 10 games at tc=10+0.1. Zero illegal-move forfeits, zero time losses; every game ended in a real mate, 3-fold repetition, or insufficient material. 'grep -c forfeit|illegal|time' over the 2+0.05 PGN returns 0.

AC #5 - Four unit tests cover the paths: zero_time_limit_still_returns_a_legal_move, near_zero_time_budget_completes_the_guaranteed_ply, aborts_are_suppressed_only_until_the_first_ply_completes, time_limited_search_honors_the_budget_after_the_guaranteed_ply. All pass and each asserts move legality via position.valid_move rather than mere presence.

Verification:
- cargo fmt --all --check: clean
- cargo clippy --workspace --lib --bins --tests: no errors (pre-existing warnings only)
- cargo test --workspace --lib --bins --tests: 111 passed, 0 failed, 1 ignored
- cargo build --release: ok
- Manual UCI boundary matrix (above): all legal, no 0000 in non-terminal positions
- FastChess 2+0.05 x40 and 10+0.1 x10: 0 illegal, 0 time forfeits
- cargo bench --bench search (A/B base vs target): target [44.8 / 46.0 / 48.0] us, base [48.3 / 52.0 / 59.2] us. No regression. perft/movegen benches are unaffected by construction (benches/perft.rs uses engine::perft::Perft and benches/movegen.rs uses core movegen; neither links through Search::stopping), and the machine was not idle (competing self-play processes from another worktree, load ~9.8), so absolute numbers are indicative only.

Non-blocking observations for the human, not deferred blocking findings and no follow-up task created:
1. time.rs allocation is unchanged, so at 2+0.05 the per-move budget still saturates to 0 for roughly the first 13 moves; the engine plays those instantly at depth 1 and only reaches ~45ms searches once increment has banked clock. This is a pre-existing strength/tuning defect, is present on master, causes no forfeit or overrun, and was explicitly scoped out by the implementation plan. AC #3 as written (no illegal-move forfeits, no losses on time attributable to overrun) holds.
2. Suppressing the cancellation flag during ply 1 means a UCI 'stop' cannot interrupt the first ply. Measured at ~10-150ms including process start, so it is bounded and safe, but it is a deliberate minor deviation from prompt-stop semantics worth knowing about.
3. FastChess 'Illegal PV move' warnings on mate scores are pre-existing: reproduced on the base build (8 occurrences in 6 games at 10+0.1). They concern the info-line PV only, never the played move.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Search now guarantees a completed first ply before any abort can take effect, so a legal root move is always available. engine/src/search.rs gains Search::min_search_complete: stopping() returns false until the first iterative-deepening iteration finishes, suppressing both the cancellation flag and the time deadline; deeper iterations honor the clock as before. Verified independently: base d9a138c reproduces 'bestmove 0000' at a 2+0.05-derived go, target f4a4643 returns legal moves at zero, 1ns, movetime 0 and 1ms budgets and on an immediate stop, across startpos, Kiwipete, and endgame FENs. FastChess self-play 40 games at 2+0.05 and 10 games at 10+0.1: zero illegal moves, zero time forfeits, all terminations normal. cargo fmt --all --check clean, cargo clippy no errors, cargo test --workspace --lib --bins --tests 111 passed / 0 failed. Search hot-path A/B (cargo bench --bench search) showed no regression.
<!-- SECTION:FINAL_SUMMARY:END -->
