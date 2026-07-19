---
id: TASK-1.5
title: Complete the browser game experience and integration hardening
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-19 01:59'
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
- [x] #1 The responsive application lets the user start a game as White or Black, select the supported engine limit, undo or restart, flip the board, and quit the UI process
- [x] #2 The companion panel presents SAN move history, whose turn it is, game result, engine thinking state, evaluation, depth, nodes, NPS, and principal variation without overwhelming the board
- [x] #3 Reloading or reconnecting reconstructs the current authoritative game without duplicating a move or search
- [x] #4 The UI gives clear recoverable feedback for rejected moves, lost connections, server errors, and occupied fixed ports
- [x] #5 A complete game can be played through checkmate from `seaborg --ui` without console errors or an external network request
- [x] #6 Automated and documented manual checks cover desktop and narrow layouts, both player colours, promotion, castling, en passant, terminal states, reload during search, and reduced-motion behavior
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

Rework of review attempt 1 (target f3052af).

Resolved REV-1-01 — a failed quit branded a live server as stopped.
postCommand collapsed a server refusal and a transport failure into the same null return, so quit could not distinguish them and treated every outcome as a successful stop. Commands now report a CommandOutcome of ok/rejected/unreachable. quit enters the terminal stopped state only when the request was accepted or never came back — a socket dropping mid-request is the ordinary end of a real shutdown — and on a refusal rolls 'quitting' back to false, leaves the controls live, and keeps the message sendCommand already wrote. postCommand keeps its null-returning shape, so playMove and sendControl are unchanged. The rule itself lives in format.ts as quitEndsTheSession so it is unit tested rather than buried in a DOM handler.
Verified against the running binary: an unauthenticated quit is refused 403 invalid_token with /api/state still answering 200, and an authenticated quit still answers 200 {quitting:true}, prints 'Seaborg UI stopped.', and closes the port.

Resolved REV-1-02 — no documented manual check procedure.
Root cause was not a missing document. docs/browser-ui-manual-checks.md was written during the first attempt but .gitignore line 3 ignored /docs, so it was never staged and never appeared in the diff the reviewer read; docs/strength-testing.md is tracked only because it was force-added earlier. Nothing in the repository or any worktree relied on the rule, so on the human's decision it is removed and docs/ becomes a normal tracked directory, which also prevents the same silent loss recurring. The procedure covers desktop and narrow layouts, both colours, promotion, castling, en passant, terminal states, reload during search, reduced motion, keyboard and assistive technology, and names which items automated tests cover instead. A thirteenth check for the refused-quit path above was added.

Non-blocking reviewer notes, all three addressed as inaccurate comments rather than defects:
- The engine panel kept the previous search's eval/depth/nodes/NPS/PV on screen under a live 'Thinking' chip until the first progress event arrived, which its own retention comment did not justify. The panel now clears once per search, keyed on searchId rather than on progress being null so a repaint before the first progress event does not re-clear.
- The principal_variation_san doc comment claimed undo cancels the search before touching the position. undo actually unmakes at game.rs:242 before cancelling at game.rs:245; the invariant holds because every mutating entry point takes &mut self under the session mutex and the search thread reads an independent clone. The comment now says that.
- formatCount rendered 999,999 as '1000k' and 999,999,999 as '1000M', overflowing the stable width its doc describes. Units are now chosen from what the value rounds to, so those read '1.0M' and '1.0B'.
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

author: @claude
created: 2026-07-19 01:40
---
Implementation handoff
Branch: task-1.5-browser-game-experience
Worktree: /Users/seabo/seaborg-worktrees/task-1.5-browser-game-experience
Base: 4d48c35917a2955550f5a0bbc6a0120d3b0cc957
Implementation target: 758cf9737e0a2b4be3e34c3ffc0e1c2f7fc35c5f
Resolved findings: REV-1-01, REV-1-02
Verification:
- cargo fmt --check: pass, exit 0
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, exit 0, zero warnings. Re-run with a clean CARGO_TARGET_DIR to rule out cached results; still zero.
- cargo test --workspace: pass, 225 passed / 0 failed / 2 ignored (the 2 ignored are pre-existing)
- tsc 5.9.3 -p engine/src/ui/frontend/tsconfig.json then git diff --exit-code engine/src/ui/assets: pass, regenerated JS byte-identical to the committed output
- node --test board.test.mjs format.test.mjs: pass, 16 passed / 0 failed (was 15; the new one is the rejected-quit rule)
- Live refused quit against the release binary on 127.0.0.1:39217: POST /api/quit without a token answered 403 {"error":"invalid_token"} and GET /api/state still answered 200, i.e. the exact state REV-1-01 said the page must not call stopped
- Live accepted quit on the same server: 200 {"quitting":true}, 'Seaborg UI stopped.' printed, port closed. The happy path is unregressed.
Known failures: none.

Two notes for the reviewer.

1. The REV-1-01 fix is asserted at the level of the rule, not the DOM handler. quitEndsTheSession in format.ts is unit tested for all three outcomes, and quit() is a single call to it; the server half is already covered by quit_needs_the_session_token. What is not automated is the two meeting in a browser, because app.ts runs its DOM and EventSource setup at module load and cannot be imported under node --test without restructuring it into an init function. That restructuring is not in this task's scope, so manual check 13 covers the seam instead. Flagging it rather than implying the path is fully automated.

2. REV-1-02 changes .gitignore, which is repository-wide and outside this task's strict surface. The manual check document existed during attempt 1 and was silently swallowed by the '/docs' ignore rule, which is why no document appeared in the diff. I put the option to the human rather than deciding it, and the decision was to un-ignore docs/ rather than force-add the single file, so the trap does not catch the next writer. docs/strength-testing.md was already tracked via an earlier force-add and is unaffected; no worktree had any other content under docs/.
---

author: @claude
created: 2026-07-19 01:59
---
Review attempt: 2
Reviewed branch: task-1.5-browser-game-experience
Reviewed implementation: 758cf9737e0a2b4be3e34c3ffc0e1c2f7fc35c5f
Verdict: approved

Both attempt-1 blocking findings are resolved, and I confirmed each independently rather than
from the handoff.

REV-1-01 resolved. sendCommand now reports ok/rejected/unreachable and quit consults
quitEndsTheSession, so the terminal stopped state is entered only on acceptance or on a request
that never returned. A refusal rolls 'quitting' back to false, repaints, and returns before the
event stream is closed, so the controls stay live and the message sendCommand wrote survives.
I reproduced the exact scenario the finding named against the release binary on 127.0.0.1:39412:
an untokened POST /api/quit answered 403 {"error":"invalid_token"}, GET /api/state still
answered 200, and the terminal had not printed 'Seaborg UI stopped.'. The accepted path is
unregressed: a tokened quit answered 200 {"quitting":true}, the port closed, and the terminal
printed 'Seaborg UI stopped.'. The rule is unit tested for all three outcomes.

REV-1-02 resolved. docs/browser-ui-manual-checks.md is now tracked and covers every item
acceptance criterion #6 names — desktop and narrow layouts, both colours, promotion, castling,
en passant, terminal states, reload during search, and reduced motion — plus keyboard and
assistive technology and a thirteenth check for the refused quit above, naming for each what to
do, what a pass looks like, and which items automated tests cover instead. The root cause was
the root-anchored '/docs' ignore rule, removed here. I checked that change is safe: '/docs' never
matched backlog/docs, nothing in the repository generates into docs/, and the only other entry is
the already-tracked docs/strength-testing.md.

The three non-blocking notes from attempt 1 were all addressed. The engine panel now clears once
per search keyed on searchId rather than on progress being null; the principal_variation_san doc
comment now states the ordering the code actually has (undo unmakes before it cancels, and the
invariant holds because the search thread reads an independent clone under the session mutex);
and formatCount chooses its unit from what the value rounds to, so 999,999 reads as 1.0M and
999,999,999 as 1.0B, which I verified by hand against the implementation.

Acceptance criteria evidence:
- #1 index.html carries new-white, new-black, engine-limit, undo, restart, flip, and quit; the
  limit endpoint is covered by four server tests and quit was driven live.
- #2 the panel carries history, turn-status, engine-state, evaluation, depth, nodes, nps, hash,
  and variation; SAN history and the SAN variation are asserted server-side.
- #3 reloading_during_a_search_reconstructs_the_game_without_duplicating_it holds revision 1, the
  same searchId, and history length 1 across three reloads, then a single reply.
- #4 describeCommandError covers every code the server returns, 5xx is distinguished from a
  refusal, and I confirmed live that a second instance on an occupied fixed port exits 1 naming
  the port and suggesting --ui-port.
- #5 I drove a complete game over the command surface myself: checkmate, winner black, 48 plies,
  0 legal moves, and a further move refused 409 game_over. Every asset reference is a same-origin
  relative path (/app.js, /style.css, /api/state, /api/events), so no external request is possible.
- #6 the manual check document above, alongside the automated coverage it names.

Verification (all run by me on the target, not taken from the handoff):
- cargo fmt --check: pass, exit 0
- cargo clippy --workspace --all-targets --all-features -- -D warnings with a fresh
  CARGO_TARGET_DIR: exit 0, zero occurrences of 'warning' in the log
- cargo test --workspace: exit 0, 225 passed / 0 failed / 2 ignored (driver_panic_process_probe
  and tt::tests::make_tt, both pre-existing)
- tsc 5.9.3 -p engine/src/ui/frontend/tsconfig.json then git diff --exit-code engine/src/ui/assets:
  both exit 0, so the committed JS is byte-identical to a real recompile of the TypeScript
- node --test board.test.mjs format.test.mjs: 16/16
- No #[allow] is added anywhere in the diff
- The diff touches no search, movegen, position, or evaluation file, so the hot-path benchmarks
  were not required
- 758cf973 is an ancestor of the branch tip and git diff --stat 758cf973..HEAD touches only
  backlog/tasks/task-1.5, so the target is immutable

One note carried forward for the merge, not a blocking finding. The REV-1-02 fix edits .gitignore,
which is repository-wide and wider than this task's surface. The implementation notes record that a
human chose to un-ignore docs/ rather than force-add the single file; I can see that decision
recorded but cannot verify it independently, so I am surfacing it rather than treating it as
settled. The change itself is benign on the evidence above.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed the browser game experience: a companion panel (SAN scoresheet, turn, result, engine state, White-relative evaluation, depth, nodes, NPS, hash, SAN principal variation), controls for new game as either colour, restart, undo, flip, a bounded thinking limit, and quit, plus recoverable feedback for rejected moves, lost connections, server errors, and occupied ports. Server gained /api/engine-limit and /api/quit, and UiHandle and the quit route now share one ShutdownSignal. Verified on 758cf973 by cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings on a clean CARGO_TARGET_DIR (zero warnings), cargo test --workspace (225 passed / 0 failed / 2 pre-existing ignored), tsc 5.9.3 with the regenerated JS byte-identical to the committed assets, node --test (16/16), and live runs against the release binary covering a refused quit that leaves the server running, an accepted quit that shuts it down cleanly, an occupied fixed port, and a complete game driven to checkmate in 48 plies with a further move refused 409 game_over.
<!-- SECTION:FINAL_SUMMARY:END -->
