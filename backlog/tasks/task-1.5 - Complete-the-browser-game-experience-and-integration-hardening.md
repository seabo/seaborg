---
id: TASK-1.5
title: Complete the browser game experience and integration hardening
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-19 01:31'
labels: []
dependencies:
  - TASK-1.4
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Finish the application around the chessboard, integrate game and engine information, and verify the complete CLI-to-browser playing flow across supported interaction and lifecycle edge cases.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The responsive application lets the user start a game as White or Black, select the supported engine limit, undo or restart, flip the board, and quit the UI process
- [ ] #2 The companion panel presents SAN move history, whose turn it is, game result, engine thinking state, evaluation, depth, nodes, NPS, and principal variation without overwhelming the board
- [ ] #3 Reloading or reconnecting reconstructs the current authoritative game without duplicating a move or search
- [ ] #4 The UI gives clear recoverable feedback for rejected moves, lost connections, server errors, and occupied fixed ports
- [ ] #5 A complete game can be played through checkmate from `seaborg --ui` without console errors or an external network request
- [ ] #6 Automated and documented manual checks cover desktop and narrow layouts, both player colours, promotion, castling, en passant, terminal states, reload during search, and reduced-motion behavior
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework of review attempt 1 findings on target f3052af.

1. REV-1-01: distinguish a command rejection from a transport failure in postCommand, which currently collapses both to null. quit() must enter the stopped state only when the quit was accepted (2xx) or the request failed at the transport layer (the genuine shutdown signal); on a 4xx/5xx rejection it must roll quitting back to false, leave the controls live, and keep the message postCommand produced. Add a regression test for the rejected-quit path.
2. REV-1-02: root cause is .gitignore line 3 '/docs', which silently swallowed docs/browser-ui-manual-checks.md — the procedure was written last attempt but never committed, so the reviewer correctly saw no doc in the diff. Per human decision, remove the '/docs' ignore rule so docs/ is a normal tracked directory, and commit the manual-check procedure. Extend it with the rejected-quit case from step 1.
3. Address the reviewer's three non-blocking notes, each of which is an inaccurate comment or a stated-invariant violation rather than a defect: the engine-panel retention justification (app.ts:363-367), the undo ordering claim (game.rs:325-326), and formatCount unit rollover at 999,999 / 999,999,999.
4. Recompile the frontend with a real tsc, confirm the committed JS is byte-identical, and run all repository-required Rust gates before handing a new immutable target to review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Controller and protocol:
- GameController gained a runtime-settable search limit that applies from the next engine turn (a running search keeps the limit it started with, so adjusting a setting never discards work or changes the move about to be played). The limit is published on GameSnapshot as engineLimit.
- Thinking snapshots now carry principalVariationSan beside SearchProgress. SAN is derived in the controller because only it holds the position the reported moves are read against; the line is truncated at the first move that is not legal there, since a transposition-table move can survive into a line it cannot be played in.
- New endpoints: POST /api/engine-limit (kind time|depth with a bounded value; Infinite is deliberately unreachable because it would never produce a reply) and POST /api/quit (answers 200 before stopping, so the browser sees acceptance rather than a dropped connection). UiHandle and the quit route now share one ShutdownSignal instead of open-coding the stop sequence twice.
- format.js is a sixth embedded asset.

Browser:
- Companion panel showing SAN move history as a numbered scoresheet, turn, result, engine idle/thinking, evaluation, depth, nodes, NPS, hash, and the SAN principal variation. Evaluation is normalised to White, since the engine scores relative to the side it searches for.
- Controls: new game as White or Black, restart (a new game on the side already being played, so no new command was needed), undo, flip board, a thinking-limit select, and quit.
- Recoverable feedback: server codes are rendered as sentences, 5xx is distinguished from a rejection, connection loss is announced and cleared on recovery, and quit puts the page into a terminal state that stops reconnecting.
- Pure presentation rules were extracted into format.ts so they are unit testable without a DOM.

Two defects were found by driving the real page in Chrome and are fixed here:
- render() returned early when a newer revision had already arrived, skipping the whole frame rather than just the stale state. The pending-command flag therefore cleared without being painted, leaving the status on 'Sending move…' with controls disabled until an unrelated event repainted. Now the stale snapshot is still not adopted but the frame is always painted (shouldAdopt, with a regression test).
- Repainting replaces every square and the replacements are disabled while the engine thinks, so a keyboard user was dropped onto the document on selecting a piece and again on every engine turn. The board now tracks that it owns the keyboard and takes focus back once a square can hold it.

Out of scope, observed and confirmed pre-existing: the engine misses a mate in one (1.f3 a6 2.g4 missing 2...Qh4#). Reproduced through plain --uci, which uses none of this task's code, so it is a search issue rather than a UI one. Not fixed here.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 01:05
---
Implementation handoff
Branch: task-1.5-browser-game-experience
Worktree: /Users/seabo/seaborg-worktrees/task-1.5-browser-game-experience
Base: 4d48c35917a2955550f5a0bbc6a0120d3b0cc957
Implementation target: <handoff commit>
Resolved findings: none (first implementation attempt)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 225 passed / 0 failed / 2 ignored (the 2 ignored are pre-existing)
- tsc -p engine/src/ui/frontend/tsconfig.json then git diff --exit-code engine/src/ui/assets: pass, regenerated JS byte-identical to the committed output
- node --test board.test.mjs format.test.mjs: pass, 15 passed / 0 failed
- Real-browser run of seaborg --ui at 1440x1000 and 390x844: board square with 64 equal squares and no horizontal overflow, no console errors, and no off-origin requests (12 resources, all same-origin). Keyboard-only move played end to end with focus retained across the engine turn. Flip, restart, undo, limit select, and new game as either colour all exercised.
- Complete game to checkmate through the running server: 60 plies, final gameStatus {kind: checkmate, winner: black}, 0 legal moves, and a further move refused 409 game_over.
- Quit against the real process: 200 {quitting:true}, listener stopped, 'Seaborg UI stopped.' printed, exit status 0.
- Occupied fixed port: second instance on the same port exits 1 naming the port and suggesting --ui-port.
Known failures: none.

Note for review: the engine misses a mate in one (1.f3 a6 2.g4 missing 2...Qh4#). I confirmed this reproduces through plain --uci, which uses none of this task's code, so it is pre-existing search behaviour and is not addressed here.
---

author: @claude
created: 2026-07-19 01:05
---
Correction to the handoff above: the implementation target is 29fa503b, this task-only handoff commit. The tree at 29fa503b is identical to f3052af except for the Backlog task file, so every verification result recorded above holds at the target.
---

author: @claude
created: 2026-07-19 01:05
---
Disregard the two sha corrections above; they chased their own commits. Settling it:

Implementation target: f3052af30e8f2db2c7e2264d8ba96f154f775c26 — the last commit containing implementation changes, and the commit every verification result in the handoff was measured at.

Every commit after f3052af on this branch touches only backlog/tasks/task-1.5, which the following verifies:
  git diff --stat f3052af..HEAD
Review the branch diff 4d48c35..f3052af for the implementation.
---

author: @claude
created: 2026-07-19 01:10
---
Retraction: the 'engine misses a mate in one' note in my handoff is wrong. There is no such bug, and no follow-up task should be created for it.

I claimed that after 1.f3 a6 2.g4 the engine missed 2...Qh4#. Qh4 is not legal in that position: the d8-h4 diagonal runs d8-e7-f6-g5-h4 and e7 still holds a black pawn, so the queen cannot leave d8 at all. The engine played a6a5 because the mate I asserted did not exist. I asserted it without checking that the queen could reach h4.

The real fool's mate line is 1.f3 e5 2.g4, where e5 opens the diagonal. In that position (rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2) the engine finds it immediately:
  go depth 1     -> info depth 1 score mate 1 nodes 33 pv d8h4 / bestmove d8h4
  go movetime 1000 -> info depth 255 score mate 1 nodes 7907 pv d8h4 / bestmove d8h4

Mate detection and mate scoring are correct. The supporting 'go movetime 1000 returned only depth 1, 20 nodes' observation was also an artifact: I piped quit immediately after go and truncated the search. Given its time the same position reaches depth 9 over 3M nodes.

Nothing in the implementation or its verification depends on this claim; only the handoff note was wrong.
---

author: @claude
created: 2026-07-19 01:21
---
Review attempt: 1
Reviewed branch: task-1.5-browser-game-experience
Reviewed implementation: f3052af30e8f2db2c7e2264d8ba96f154f775c26
Verdict: changes_requested

The implementation is strong: the controller, protocol, and server work is correct under
adversarial tracing, all repository-required checks pass on a clean target directory, and I
independently reproduced the engine-limit, PV-SAN, reload-during-search, quit, and occupied-port
behaviour against a live server. Two blocking findings remain.

REV-1-01 [P1] A failed quit brands a live server as stopped and cannot be recovered
Location: engine/src/ui/frontend/app.ts:716-728 (quit)
Impact: Blocks acceptance criterion #4. quit() is the only caller that discards postCommand`s
  null return, contradicting the contract documented at app.ts:476-482. On any non-2xx the page
  enters a permanent false terminal state: quitting stays true, so canInteract locks the board
  and busy = commandPending || quitting disables every control including the quit button, the
  SSE stream is closed, and the accurate message postCommand just wrote is overwritten by
  "Seaborg has stopped. You can close this tab." Only a manual reload recovers. This is the one
  case where recoverable feedback matters most, and it is the case where it is suppressed.
Reproduction: Two reachable server paths, both confirmed against the running binary:
  - 403 invalid_token. handle_command authenticates before dispatch (server.rs:576-578).
    /api/events is deliberately not token-checked (server.rs:547), so a tab left open across a
    server restart reconnects its stream and reads "Connected" while every command 403s.
    Clicking Quit then reports a stopped server that is still running. Verified live:
      curl -s -w " HTTP=%{http_code}" -X POST -H "Content-Type: application/json" \
        -d "{}" http://127.0.0.1:PORT/api/quit
      -> {"error":"invalid_token"} HTTP=403, and GET /api/state still answered 200.
  - 503 too_many_connections. Refused at the accept loop before routing (server.rs:349-351,
    refuse -> ServiceUnavailable at server.rs:228-230) once MAX_CONNECTIONS = 64 is reached.
    postCommand takes its status >= 500 branch and returns null.
Expected: Enter the stopped state only when the quit was actually accepted, or when the request
  failed at the transport layer (the genuine shutdown signal already handled at app.ts:502-509).
  On a rejection, roll quitting back to false, leave the controls live, and keep the message
  postCommand produced. Cover the rejected-quit path with a test.

REV-1-02 [P2] Acceptance criterion #6 has no documented manual check procedure
Location: repository-wide; no documentation file is added or changed between 4d48c35 and f3052af
Impact: Blocks acceptance criterion #6, which requires automated AND documented manual checks
  covering desktop and narrow layouts, both player colours, promotion, castling, en passant,
  terminal states, reload during search, and reduced-motion behavior. Implementation plan step 7
  committed to adding this procedure and it is absent, so the criterion cannot be checked.
  The handoff comment records a one-off run of results rather than a repeatable procedure, and
  it does not cover promotion, castling, en passant, or reduced motion at all. Reduced motion
  and the narrow layout have no automated coverage in this task either, so nothing in the
  repository establishes how those are to be checked.
Reproduction:
  git diff --name-only 4d48c35..f3052af | grep -iE "\.md$|docs/|README"
  -> only backlog/tasks/task-1.5, i.e. no documentation was added.
Expected: A committed, repeatable manual check procedure covering every item the criterion names,
  stating for each what to do and what a pass looks like, and naming which items are covered by
  automated tests instead.

Non-blocking observations, offered as notes and not as required changes:
- engine/src/ui/frontend/app.ts:354-382. renderEnginePanel only overwrites the stats when
  progress is non-null and the PV when it is non-empty. Every search starts with progress None
  (game.rs:131-137), so the snapshot published the instant a human move lands shows the previous
  search`s eval/depth/nodes/NPS/PV underneath a live "Thinking" chip until the first progress
  event arrives. The retention comment at app.ts:363-367 justifies this by the chip reading
  "Idle", which is not the case here. The window is short but the reasoning does not cover it.
- engine/src/game.rs:325-326. The doc comment states that "undo and reset cancel the search
  before touching the position". That holds for replace_position but not for undo, which calls
  self.position.unmake_move() at game.rs:242 before self.cancel_search() at game.rs:245. The
  invariant is still observably true, because undo takes &mut self under the session mutex and
  the search thread owns an independent cloned position, so this is an inaccurate justification
  rather than a defect. Worth correcting so a later reader does not rely on an ordering the code
  does not have.
- engine/src/ui/frontend/format.ts:79-85. formatCount yields "1000k" for 999,999 and "1000M" for
  999,999,999 instead of rolling to the next unit, which costs a character of the stable width
  its own doc comment describes.

Verification (all run by me on f3052af, not taken from the handoff):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings, clean CARGO_TARGET_DIR:
  pass, exit 0, zero warnings
- cargo test --workspace: pass, 0 failed, 2 ignored (both pre-existing)
- tsc 5.9.3 -p engine/src/ui/frontend/tsconfig.json then git diff --exit-code engine/src/ui/assets:
  pass, regenerated JS byte-identical to the committed assets
- node --test board.test.mjs format.test.mjs: pass, 15/15
- Live server on 127.0.0.1:39117: engine-limit accepted and published (time 750); depth 64,
  infinite, and a missing token refused 422/422/403; /format.js served as text/javascript;
  principalVariationSan derived correctly beside its UCI line, including bxa4 and Rxa4; three
  reloads during a live search held revision 1, searchId 1, history length 1
- Live quit: unauthenticated quit refused 403 with the server surviving; authenticated quit
  answered {"quitting":true} 200, printed "Seaborg UI stopped.", and exited 0
- Occupied fixed port: second instance exited 1 naming the port and suggesting --ui-port
- Diff touches no search, movegen, or core file, so the hot-path benchmarks were not required
- No #[allow] is added anywhere in the diff; no external network reference exists in the assets
---
<!-- COMMENTS:END -->
