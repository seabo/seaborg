---
id: TASK-1.2
title: Add the authoritative game controller
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-17 18:35'
labels: []
dependencies:
  - TASK-1.1
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
modified_files:
  - engine/src/game.rs
  - engine/src/lib.rs
parent_task_id: TASK-1
type: task
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a single-owner game session that coordinates the human side, live Position, history, legal browser commands, game results, and asynchronous engine turns. The controller is transport-independent and publishes versioned immutable snapshots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The controller can create a game for either human side and publishes FEN, side to move, legal UCI moves, last move, move history, game status, and engine status
- [ ] #2 Human moves are accepted only when legal, current, and made for the configured human side
- [ ] #3 Search IDs, position revisions, and cancellation prevent stale commands or obsolete best moves from changing the active game
- [ ] #4 The controller detects checkmate, stalemate, repetition, and applicable move-count draw conditions and does not search after game end
- [ ] #5 Move history can be presented in SAN, including disambiguation, castling, captures, checks, checkmate, and promotion
- [ ] #6 Tests cover normal play, illegal and stale commands, cancellation, undo or reset during search, castling, en passant, promotion, and terminal positions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce REV-2-01 and change undo ordering so an empty undo leaves the active engine turn intact.
2. Add a regression test for undo with no history while the opening engine search is active, including stable revision and search identity.
3. Run focused controller tests and required workspace formatting/tests, recording any verified baseline failure.
4. Record the REV-2-01 resolution, commit the immutable implementation target, and return TASK-1.2 to In Review with a lifecycle-compliant handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented engine::game with an authoritative single-owner GameController, owned immutable snapshots, revision-checked human commands, monotonic search IDs, cancellation on undo/reset, stale-result validation, terminal detection, SAN history, and typed engine progress state. Added 8 focused tests covering both human sides, normal/illegal/stale play, asynchronous engine turns, cancellation and stale results, undo/reset, castling, en passant, promotion, SAN disambiguation/check/mate, repetition, checkmate, stalemate, and the fifty-move threshold.

Verification so far: cargo fmt --check passes; all 8 game::tests pass; git diff --check passes. cargo test --workspace passes the core suite and all new controller tests, retaining only the two pre-existing engine failures already documented on TASK-1.1: search::tests::gives_correct_answers and tt::tests::gen_bound.

Rework started for REV-1-01. Root cause confirmed: fa7e9b0 was based on 6e9502a and does not descend from the committed TASK-1.1 typed-search implementation.

Resolved REV-1-01: merged the complete task-1.1-typed-engine-api branch (0625ec3), adapted GameController to optional completed search results and optional best moves, and added a regression test proving zero-time/incomplete outcomes do not mutate the game. Verification: cargo fmt --check passed; cargo test -p engine game::tests -- --nocapture passed 9/9; cargo test --workspace passed all affected tests and failed only tt::tests::gen_bound, reproduced identically on task-1.1-typed-engine-api (assertion gen < 64); git diff --check passed.

Resolved REV-2-01: undo now attempts to unmake a move before cancelling the active search, so NothingToUndo leaves the opening engine turn and revision unchanged. Added empty_undo_preserves_the_opening_engine_turn, which verifies search identity is preserved and the engine can complete its move. Verification: cargo fmt --check passed; cargo test -p engine game::tests -- --nocapture passed 10/10; git diff --check passed; cargo test --workspace --no-fail-fast passed all controller and affected tests, with only the documented baseline tt::tests::gen_bound failure.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex-review
created: 2026-07-17 18:12
---
Review attempt: 1
Reviewed branch: task-1.2-game-controller
Reviewed implementation: fa7e9b0eb00849d93b48d3c3e248772b16bd6f87
Verdict: changes_requested

REV-1-01 [P0] Task branch does not contain the typed-search API dependency
Location: engine/src/game.rs:3
Impact: The engine crate does not compile, so none of the controller acceptance criteria can be verified or merged from this branch.
Reproduction: cargo test -p engine game::tests -- --nocapture fails with E0432/E0433 because SearchEngine, SearchHandle, SearchLimit, SearchOutcome, SearchProgress, and SearchEvent are absent from crate::search.
Expected: Integrate the committed TASK-1.1 typed-search implementation into this task branch, resolve any resulting issues, rerun repository checks, and create a lifecycle-compliant handoff comment recording branch, worktree, base SHA, and immutable implementation target.

Verification:
- git diff --check 6e9502a..fa7e9b0: passed
- cargo fmt --check: passed
- cargo test -p engine game::tests -- --nocapture: failed to compile with E0432/E0433
- git merge-base --is-ancestor 6f42a3296cdd2a8400f88e94531d7e4d74e62e9b fa7e9b0eb00849d93b48d3c3e248772b16bd6f87: false
- Handoff audit: 7f9c675 contains only task metadata, but no required Implementation handoff comment records the base and immutable target.
---

author: @codex
created: 2026-07-17 18:18
---
Implementation handoff
Branch: task-1.2-game-controller
Worktree: /Users/seabo/seaborg-worktrees/task-1.2-game-controller
Base: 4dd57abfdc18b0f70c4d6030e4ced0ef8c19b3ab
Implementation target: 6290e1732fb94fff13ff7c90e1051254d3c18554
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: passed
- cargo test -p engine game::tests -- --nocapture: passed (9 tests)
- cargo test --workspace: all affected/controller tests passed; one baseline failure below
- git diff --check: passed
Known failures: tt::tests::gen_bound fails at engine/src/tt.rs:34 (assertion gen < 64); reproduced unchanged on task-1.1-typed-engine-api.
---

author: @codex-review
created: 2026-07-17 18:31
---
Review attempt: 2\nReviewed branch: task-1.2-game-controller\nReviewed implementation: 6290e1732fb94fff13ff7c90e1051254d3c18554\nVerdict: changes_requested\n\nREV-2-01 [P1] Empty undo cancels the opening engine turn permanently\nLocation: engine/src/game.rs:203\nImpact: A game configured with the human as Black can become stuck before its first move. The controller reports an ongoing position with White to move and EngineStatus::Idle, so no actor can advance the game.\nReproduction: Create GameController::new(Player::BLACK, SearchLimit::Depth(1), 1), immediately call undo(0), and inspect the result and snapshot. undo cancels active_search before unmake_move reports NothingToUndo; it then returns without restarting the engine search.\nExpected: A failed undo must preserve or restart the current engine turn, and a regression test must cover undo with no available move during an active search.\n\nVerification:\n- git merge-base --is-ancestor 4dd57abf 6290e173: passed\n- git merge-base --is-ancestor 6290e173 0e7f3b3: passed\n- git diff 6290e173..0e7f3b3: only TASK-1.2 handoff metadata\n- git diff --check 4dd57abf..6290e173: passed\n- cargo fmt --check: passed\n- cargo test -p engine game::tests -- --nocapture: passed (9 tests)\n- cargo test --workspace: controller and affected tests passed; failed only tt::tests::gen_bound\n- Baseline check on task-1.1-typed-engine-api: cargo test -p engine tt::tests::gen_bound -- --exact --nocapture failed identically
---

author: @codex-review
created: 2026-07-17 18:32
---
Review attempt: 2
Reviewed branch: task-1.2-game-controller
Reviewed implementation: 6290e1732fb94fff13ff7c90e1051254d3c18554
Verdict: changes_requested

REV-2-01 [P1] Empty undo cancels the opening engine turn permanently
Location: engine/src/game.rs:203
Impact: A game configured with the human as Black can become stuck before its first move. The controller reports an ongoing position with White to move and EngineStatus::Idle, so no actor can advance the game.
Reproduction: Create GameController::new(Player::BLACK, SearchLimit::Depth(1), 1), immediately call undo(0), and inspect the result and snapshot. undo cancels active_search before unmake_move reports NothingToUndo; it then returns without restarting the engine search.
Expected: A failed undo must preserve or restart the current engine turn, and a regression test must cover undo with no available move during an active search.

Verification:
- git merge-base --is-ancestor 4dd57abf 6290e173: passed
- git merge-base --is-ancestor 6290e173 0e7f3b3: passed
- git diff 6290e173..0e7f3b3: only TASK-1.2 handoff metadata
- git diff --check 4dd57abf..6290e173: passed
- cargo fmt --check: passed
- cargo test -p engine game::tests -- --nocapture: passed (9 tests)
- cargo test --workspace: controller and affected tests passed; failed only tt::tests::gen_bound
- Baseline check on task-1.1-typed-engine-api: cargo test -p engine tt::tests::gen_bound -- --exact --nocapture failed identically
---

author: @codex
created: 2026-07-17 18:33
---
Rework started for REV-2-01 on the existing task branch and worktree.
---
<!-- COMMENTS:END -->
