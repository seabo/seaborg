---
id: TASK-34
title: >-
  Engine self-play under a UCI runner is unstable: intermittent search hang,
  illegal PV moves, EOF null move
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 00:25'
updated_date: '2026-07-18 12:05'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: high
type: bug
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
While validating the TASK-27 strength-regression tooling against a real FastChess v1.5.0 build, driving seaborg vs seaborg exposed several engine-side robustness defects that are independent of TASK-27 (the orchestrator is runner-agnostic and correct). These must be fixed before seaborg can be strength-tested by self-play, but they do not block landing the tool.

Observed against a release build driving FastChess (fastchess -engine cmd=seaborg args=-u ... -each proto=uci depth=4 ...):

1. Intermittent search/UCI deadlock. In some self-play games the seaborg process goes idle mid-game (near-zero CPU, sleeping) and never returns a 'bestmove', hanging the match indefinitely. It is nondeterministic: a 16-game depth=4 match completed in ~5s on one run, while a single game deadlocked on a later run with the same flags. This points to a race or deadlock in the search/stop/UCI-I/O handling rather than a specific position.

2. Illegal moves in the reported principal variation. FastChess repeatedly emits 'Warning; Illegal PV move - move XXXX from <engine>' during otherwise-legal games, so the PV that seaborg reports over UCI (info ... pv ...) contains illegal moves. The game continues, but PV output is wrong.

3. Search aborts to the null move on stdin EOF. When stdin is closed while a search is running (e.g. a fire-and-forget 'uci/isready/go/quit' pipe), seaborg returns 'bestmove 0000' instead of the best/legal move found so far. TASK-27's preflight was reworked to keep stdin open as a workaround, but the engine should still return a legal move.

Related: TASK-32 covers the distinct time-allocation defect (null move / illegal move at starved fast time controls). These robustness issues (deadlock, illegal PV, EOF handling) are separate from time allocation and from TASK-27.

### Scope of this ticket: investigate and spec, do not fix

These are serious, likely-interacting concurrency and correctness defects in the search/stop/UCI-I/O path. They are too high-risk to attempt to fix in a single implementation pass. Implementing this ticket therefore means **investigating each of the failures above and producing fresh, well-scoped tickets that spec the fix for each**, not landing engine fixes here.

Concretely, the work is to:

- Reproduce and root-cause each of the three failure modes (intermittent search/UCI deadlock; illegal PV moves; EOF null-move abort), gathering enough evidence (repro conditions, stack/state at hang, offending positions/PVs, relevant code paths) to characterize the underlying defect rather than the symptom.
- Determine whether the failures are independent or share a common cause (e.g. the stop/abort mechanism interacting with UCI I/O), and note any coupling with TASK-32 (time allocation) so overlapping fixes are not duplicated.
- Write one or more new tickets that spec the solution for each defect (or each root cause), each with its own acceptance criteria, so they can be implemented and reviewed independently and safely.

No engine code fixes should land under this ticket; its deliverable is the investigation findings plus the fresh implementation tickets.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 This ticket produces investigation findings, not engine fixes: no changes to engine search/stop/UCI-I/O code land under it
- [ ] #2 Each of the three failure modes (intermittent search/UCI deadlock; illegal PV moves; EOF null-move abort) is reproduced and root-caused, with documented evidence (repro conditions, captured state at the failure, offending positions/PVs, and the relevant code paths)
- [x] #3 The investigation determines whether the failures are independent or share a common root cause, and records any coupling with TASK-32 (time allocation) so overlapping fixes are not duplicated
- [ ] #4 One or more fresh, well-scoped implementation tickets are created that spec the fix for each defect (or root cause), each with its own acceptance criteria so it can be implemented and reviewed independently; those tickets carry forward the original fix-level requirements (no hang under repeated self-play, only-legal PV moves, legal best-so-far move on stdin EOF, and regression coverage of the stop/abort and EOF paths)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Merge current master into the task branch; resolve the task-34 and task-32 Comments-block conflicts by preserving both sides (done: merge commit 80a4af6).
2. Re-verify Defect 3 (EOF null move) against the merged TASK-32 code on this branch: run the isolated fixed-depth reproducer and the original depth-25 reproducer. Record evidence.
3. If Defect 3 no longer reproduces, retire TASK-37 with that evidence rather than root-causing it; update the TASK-32 cross-reference comment accordingly and confirm no fix-level requirement is dropped (legal best-so-far on stdin EOF is now satisfied and covered by TASK-32's tests).
4. Record TASK-39 coordination (UCI stop responsiveness under the abort-suppressed window) on TASK-34 and TASK-35, since it shares the stop/abort area.
5. Fix the ordinal collision: TASK-35/36 carry 38000/39000, colliding with master's TASK-38/39.
6. Update doc-2, task notes and the final summary to reflect the narrowed scope; re-run cargo fmt --check and cargo test --workspace; hand off to review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Investigation complete; no engine code changed under this ticket (base..target touches only backlog/). Findings recorded in doc-2 (backlog/docs/doc-2).

All three failure modes were reproduced and root-caused:
- Defect 3 (EOF null move): originally reproduced deterministically on master d9a138c ('printf uci/isready/go depth 25 | seaborg -u' -> bestmove 0000 from startpos, which has 20 legal moves). Root cause: EOF cancels the search before a depth completes; iterative_deepening records no result -> Cancelled(None) -> format_search_outcome emits 0000.
- Defect 2 (illegal PV): FastChess depth=4 self-play flags 'Illegal PV move - move c5f8' for 'pv d7f8 g6a6 f8g6 c5f8' (score mate -2), offending position FEN 8/3n1P2/6R1/4k1P1/P1Q5/8/4N3/4K3 b - - 0 53 (cold TT, go depth 4); PV plies 1-3 legal, ply 4 illegal. Best move (first ply) is always legal, so this is an info-line PV defect, not a move-selection defect. Root cause: the triangular PVTable updated on fail-high/cutoff nodes (search.rs Step 22) splices stale sibling rows via copy_within; mate/leaf handling compounds it.
- Defect 1 (completion deadlock): reproduced under debug-build self-play (concurrency>=8); all slots freeze, engines idle at ~0% CPU, no bestmove, no panic. Thread samples at the hang show the driver parked in crossbeam select! on the active-search branch while the search worker thread has already exited -> lost channel-disconnect wakeup; finish_search never runs.

REWORK (after merge attempt 1 ejected on a textual conflict with master c851bba):

Merged master into this branch (merge commit 80a4af6), resolving the task-34 and task-32 Comments-block conflicts by preserving both sides. No engine source was involved on either side of either conflict.

Defect 3 was re-verified against the merged TASK-32 code (release build d6c5679) and NO LONGER REPRODUCES. TASK-32's Search::min_search_complete suppresses both the time deadline and the cancellation flag until ply 1 completes; EOF reaches the search through that same cancellation flag (engine.rs:90 Input::Closed -> stop_search -> cancel()), so a legal root move is always recorded first. Five EOF variants now all return legal moves (go depth 25, go depth 8, Kiwipete go depth 20, go infinite, go depth 25 + quit); an abort after ply 1 returns the last completed iteration's move (depth-10 result after ~3s of go infinite); terminal positions still correctly emit bestmove 0000. Evidence table in doc-2.

Consequently TASK-37 was NARROWED to regression coverage only (driver-level EOF path plus terminal-position case, tests only, no engine change; priority high -> medium) rather than retired, because TASK-32's unit tests pin the search-level abort paths but nothing exercises the driver-level EOF path end to end. Retiring it outright would have left this ticket's AC #4 requirement to carry forward 'legal best-so-far move on stdin EOF, and regression coverage of the stop/abort and EOF paths' without a home.

Defect 2 is confirmed still open on current master (independently reproduced by the TASK-32 reviewer: 33 'Illegal PV move' emissions in a 6-game match at tc=10+0.1, on both the TASK-32 base and branch). Defect 1 is unaffected.

Coordination with TASK-39 (UCI stop responsiveness under the abort-suppressed window, filed on master while this was in review) recorded on TASK-35, TASK-37 and TASK-39: TASK-35 is a completion-signalling defect that occurs after the worker has exited and does not interact with the suppression window; TASK-37 and TASK-39 examine the same window from opposite directions, so any narrowing of it must preserve the EOF guarantee. TASK-37's ACs were written to assert only that a legal move is returned, not a depth or timing, so they stay valid whichever direction TASK-39 takes.

Ordinals reassigned to clear the collision with TASK-38/TASK-39 filed on master: TASK-35 38000 -> 40000, TASK-36 39000 -> 41000, TASK-37 40000 -> 42000.

Coupling: Defects 1 and 2 are independent of each other and of TASK-32. Defect 3 shared TASK-32's root cause (no guaranteed legal root move before an abort; differing only in trigger, time budget vs EOF), and the predicted single shared guarantee did in fact resolve both — implemented once, under TASK-32.

Fresh tickets: TASK-35 (Defect 1, fix), TASK-36 (Defect 2, fix), TASK-37 (Defect 3, narrowed to regression coverage).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 01:27
---
Implementation handoff
Branch: task-34-investigate-selfplay-robustness
Worktree: /Users/seabo/seaborg-worktrees/task-34-investigate-selfplay-robustness
Base: d9a138ccdeb36f39dd28fc7e19d460635ec6be29
Implementation target: f81ee2636db97be18df6cb2f327fcfe6e47645d0
Resolved findings: none (initial implementation)
Deliverable: investigation findings (backlog doc-2) + fresh tickets TASK-35 (deadlock), TASK-36 (illegal PV), TASK-37 (EOF null move, coupled to TASK-32). No engine code changed.
Verification:
- git status (excluding backlog): clean, no source/engine changes
- cargo test --workspace: ok (35 + 68 + 5 + 1 passed, 0 failed, 1 ignored)
- cargo fmt --check: clean
- Defect 3 repro: printf 'uci\nisready\ngo depth 25\n' | seaborg -u => bestmove 0000
- Defect 2 repro: FastChess depth=4 self-play => 'Illegal PV move - move c5f8'
- Defect 1 repro: debug-build self-play concurrency>=8 hangs; sample shows driver in select! with worker thread exited
Known failures: none
---

author: @codex
created: 2026-07-18 11:39
---
Review attempt: 1
Reviewed branch: task-34-investigate-selfplay-robustness
Reviewed implementation: f81ee2636db97be18df6cb2f327fcfe6e47645d0
Verdict: approved

Scope: base d9a138c..f81ee26 touches only backlog/ (doc-2, task-32 comment, task-34, and
new task-35/36/37). No engine, search, or UCI-I/O source changed, so AC #1 holds by
construction and no hot-path benchmarks were required. Handoff commit e40495b changes only
the task-34 file. No doc-id or task-id collisions with master or any active branch.

Independent verification of the findings (not merely re-reading the doc):

AC #2 - Defect 3 (EOF null move): reproduced independently.
  printf 'uci\nisready\ngo depth 25\n' | ./target/release/seaborg -u  =>  bestmove 0000
  Root cause confirmed by inspection: iterative_deepening only records a SearchResult when
  !self.stopping() (search.rs:447-457), so a cancel landing before depth 1 leaves
  Cancelled(None), which format_search_outcome maps to "bestmove 0000" (info.rs:34-38).
  Also observed the same null move when 'quit' races 'go', i.e. EOF is one trigger of a
  wider abort path - consistent with the TASK-32 coupling recorded in AC #3.

AC #2 - Defect 2 (illegal PV): reproduced independently and root cause confirmed concretely.
  fastchess -engine cmd=./target/release/seaborg args=-u name=A -engine (same) name=B
    -each proto=uci depth=4 -rounds 20 -games 2 -concurrency 4
  => 40x "Warning; Illegal PV move - move c5f8", matching doc-2's PV exactly
  ("score mate -2 ... pv d7f8 g6a6 f8g6 c5f8"). All 40 games were byte-identical
  (deterministic), so the case is reliably regenerable.
  Recovered the offending position, which doc-2 does not record (see note below):
    FEN 8/3n1P2/6R1/4k1P1/P1Q5/8/4N3/4K3 b - - 0 53
    (= position startpos moves <the 105-move game>; cold TT, go depth 4)
  python-chess 1.11.2 validation of that position: not checkmate, 6 legal moves
  [d7f8 d7b8 d7f6 d7b6 d7c5 e5f5]; PV plies 1-3 (d7f8, g6a6, f8g6) are legal and ply 4
  (c5f8) is illegal - exactly FastChess's warning. Note c5f8 is a knight move from c5,
  reachable only via the sibling branch d7c5, which directly corroborates doc-2's
  "stale sibling row spliced up via copy_within" mechanism. Confirmed structurally that
  best_move is data[depth-1], i.e. always the actual root move, so doc-2's claim that this
  is a PV-reporting and not a move-selection defect is sound.

AC #2 - Defect 1 (completion deadlock): root cause accepted, but see limitation below.
  Verified the checkable parts of the claim: crossbeam-channel is 0.5.6 as stated; the
  SearchEvent Sender is moved into the single search thread and dropped on exit with no
  retained clone (search.rs:150-172; the Worker thread type spawns no real threads), so a
  retained-sender explanation is ruled out; and output is io::stdout(), a LineWriter, so a
  buffered-but-unflushed "bestmove" is also ruled out. With those two alternatives
  eliminated, a lost disconnect wakeup is the only explanation consistent with the recorded
  thread-sample state (driver parked in select! on the active-search branch while the worker
  has exited).

Verification:
- git diff --stat d9a138c f81ee26: backlog/ only, 6 files, no engine source
- git diff --stat f81ee26 e40495b: task-34 file only
- cargo fmt --check: clean
- cargo test --workspace: ok (35 + 68 + 5 + 1 passed, 0 failed, 1 ignored)
- Defect 3 repro: bestmove 0000 (reproduced)
- Defect 2 repro: 40x "Illegal PV move - move c5f8" (reproduced); FEN + python-chess
  legality check as above
- Defect 1 repro: NOT reproduced - see limitation

Limitation recorded honestly (non-blocking):
- I could not independently reproduce the Defect 1 hang. A debug-build self-play run under
  doc-2's stated conditions (depth=5, concurrency=8, 120 games) completed all 120 games with
  no hang and no orphaned engine processes, i.e. well past the "~48-72 completed games"
  threshold doc-2 reports. This does not contradict the finding - doc-2 and TASK-34 both
  describe the hang as nondeterministic, and doc-2 itself notes a 400-game release run that
  did not hang - but the stated repro rate should be treated as optimistic. Accepting this
  does not weaken TASK-35, whose ACs are behavioural (#1 no hang under stress) and
  prescriptive (#2 do not depend on a disconnect-only completion signal); that fix is correct
  whether or not the upstream attribution to crossbeam 0.5.6 is exact, so the "upgrade
  crossbeam-channel" candidate should not be treated as a proven remedy.
- doc-2 records the offending PV but not the offending position for Defect 2. The FEN above
  is supplied here so TASK-36 AC #3 (regression test on the d7f8 g6a6 f8g6 c5f8 mate line)
  can be implemented directly; a plain position+go depth 4 with a cold TT is sufficient, no
  warm transposition-table state is needed.

AC #3 and AC #4 verified: independence/coupling is recorded in doc-2 and mirrored as a
comment on TASK-32; TASK-35/36/37 exist with their own ACs and carry forward all four
fix-level requirements (no hang under repeated self-play -> TASK-35 #1; only-legal PV moves
-> TASK-36 #1/#2; legal best-so-far on stdin EOF -> TASK-37 #1; regression coverage of the
stop/abort and EOF paths -> TASK-35 #3, TASK-36 #3, TASK-37 #4). TASK-37 records the
TASK-32 dependency.
---

author: @georgeseabridge
created: 2026-07-18 11:46
---
Cross-reference from the TASK-32 review (verified against master a04e7d5 and the TASK-32 branch build).

Failure mode 2 (illegal PV moves) is NOT fixed and still reproduces on master: 33 'Warning; Illegal PV move' emissions across a 6-game FastChess self-play match at tc=10+0.1. They cluster on mate scores (e.g. 'score mate 3 ... pv e6a6 a7b8 a6b6 b8a7 h1h5', where b8a7 is illegal). Played moves remain legal and games terminate normally, so this stays an info-line PV defect. Not caused by TASK-32: it reproduces identically on the TASK-32 base and on the TASK-32 branch, and that change touches neither PVTable population nor info.rs formatting.

Failure mode 3 (EOF null move) appears to be RESOLVED as a side effect of TASK-32, and should be re-verified rather than re-investigated from scratch. Isolated reproducer with a fixed depth so the time budget is not a confounder:
  printf 'uci\nisready\nposition startpos\ngo depth 8\n' | seaborg --uci
  master a04e7d5   -> bestmove 0000
  TASK-32 f4a4643  -> bestmove a2a3
Mechanism: on EOF the driver cancels the search, and TASK-32's Search::min_search_complete suppresses the cancellation flag until the first ply completes, so a legal root move is always recorded before any abort can take effect. Once past ply 1, an abort yields SearchOutcome::Cancelled(Some(result)) and SearchOutcome::result() returns the last completed iteration's move, so a legal move is emitted in that case too.

Recommended adjustment to this ticket's scope: confirm mode 3 against the merged TASK-32 code and, if it no longer reproduces, close it out with that evidence instead of root-causing it, leaving the investigation to focus on modes 1 and 2. Note also that TASK-39 now covers a related question in the same stop/abort area (whether TASK-32's abort-suppressed window bounds UCI 'stop' responsiveness), so coordinate with it to avoid duplicate investigation.
---

author: @codex
created: 2026-07-18 11:48
---
Merge attempt: 1
Primary tip tested: c851bba (chore(backlog): file TASK-38 time allocation and TASK-39 stop responsiveness)
Merged: 3280616 (branch tip; approved implementation target f81ee26)
Verdict: ejected - textual conflict, primary not advanced

Failing command:
  git merge --no-ff 3280616
  CONFLICT (content): Merge conflict in
    backlog/tasks/task-34 - Engine-self-play-under-a-UCI-runner-is-unstable-...md
  Automatic merge failed

The trial merge was performed on a detached HEAD at c851bba and discarded; master
is unchanged at c851bba and the primary worktree is clean. No integrated checks or
benchmarks were run, because the merge did not produce a mergeable tree.

Cause: master moved while TASK-34 was in review. c851bba modified this task's own
file, adding a @georgeseabridge comment (2026-07-18 11:46) into the COMMENTS block,
while the task branch independently added the implementation handoff and review
approval comments to the same block. The conflict is confined to this one backlog
file (frontmatter updated_date and the Comments section). No engine source is
involved on either side.

This is not merely a mechanical conflict: the incoming comment changes the scope of
the deliverable and must be reconciled deliberately, not auto-resolved.

Substantive input from the incoming master-side comment, verified against the
TASK-32 branch by its author:
- Failure mode 3 (EOF null move) is REPORTED RESOLVED as a side effect of TASK-32
  (master a04e7d5 -> bestmove 0000; TASK-32 f4a4643 -> bestmove a2a3), because
  Search::min_search_complete suppresses the cancellation flag until ply 1 completes.
  The recommendation is to confirm mode 3 against the merged TASK-32 code and close
  it out with that evidence rather than root-cause it, focusing this ticket on
  modes 1 and 2.
- Failure mode 2 (illegal PV) is confirmed NOT fixed and still reproducing on master,
  independently of TASK-32. This agrees with the review finding.
- TASK-39 (filed on master at 11:46) now covers UCI 'stop' responsiveness under
  TASK-32's abort-suppressed window, i.e. the same stop/abort area, and explicitly
  asks TASK-34 to coordinate to avoid duplicate investigation.

Rework required (for $implement, on this branch):
1. Merge current master into the task branch and resolve this task file's Comments
   block so both the master-side comment and the branch-side handoff/review comments
   are preserved.
2. Revisit Defect 3 against TASK-32's implementation. If it no longer reproduces,
   record that evidence and narrow or retire TASK-37 accordingly; TASK-37 as written
   specs the guaranteed-minimum-search fix that TASK-32 appears to have already
   implemented, so landing it unchanged risks duplicated engine work. TASK-37's own
   text already anticipates this ("if TASK-32 lands first, narrow TASK-37"), so this
   is a scope confirmation rather than a contradiction.
3. Record the TASK-39 coordination on this ticket and/or TASK-35, since TASK-39
   covers the same stop/abort area.
4. Note for the reviewer: TASK-35 and TASK-36 carry ordinals 38000 and 39000, which
   now collide with TASK-38 (38000) and TASK-39 (39000) filed on master. Cosmetic
   board-ordering issue only, but worth correcting during rework.

AC #2 and AC #4 are unchecked and the final summary cleared, because both rest on
Defect 3 being an open defect requiring its own fix ticket, which the incoming
evidence disputes. AC #1 and AC #3 are unaffected and remain proven.

Review evidence that remains valid and need not be re-derived: Defect 2 reproduces
(FEN 8/3n1P2/6R1/4k1P1/P1Q5/8/4N3/4K3 b - - 0 53, cold TT, go depth 4; PV plies 1-3
legal, ply 4 c5f8 illegal) and Defect 1 did not reproduce in an independent 120-game
debug self-play run.
---
<!-- COMMENTS:END -->
