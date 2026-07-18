---
id: TASK-32
title: Engine returns illegal null move and forfeits at fast time controls
status: In Progress
assignee:
  - '@georgeseabridge'
created_date: '2026-07-18 00:09'
updated_date: '2026-07-18 00:58'
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
