---
id: TASK-32
title: Engine returns illegal null move and forfeits at fast time controls
status: In Review
assignee:
  - '@georgeseabridge'
created_date: '2026-07-18 00:09'
updated_date: '2026-07-18 01:27'
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
- [ ] #1 For any go command, the engine returns a legal move whenever a legal move exists, including when the computed time budget is zero or near-zero (never 'bestmove 0000' in a non-terminal position)
- [ ] #2 A guaranteed-minimum search completes at least one full ply / one legal root move before any time-based abort can take effect
- [ ] #3 The search honors the allotted clock: self-play at fast time controls (e.g. 2+0.05 and 10+0.1) produces no illegal-move forfeits and no losses on time attributable to overrun
- [ ] #4 Behavior is validated with a UCI tournament runner (FastChess or cutechess-cli) playing seaborg self-play at a fast time control with zero illegal moves and zero time forfeits
- [ ] #5 Unit tests cover the zero/near-zero budget path returning a legal move and the time-abort honoring the budget
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
<!-- COMMENTS:END -->
