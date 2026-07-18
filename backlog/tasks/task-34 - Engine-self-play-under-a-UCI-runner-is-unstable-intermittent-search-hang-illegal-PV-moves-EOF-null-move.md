---
id: TASK-34
title: >-
  Engine self-play under a UCI runner is unstable: intermittent search hang,
  illegal PV moves, EOF null move
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 00:25'
updated_date: '2026-07-18 01:27'
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
- [ ] #1 This ticket produces investigation findings, not engine fixes: no changes to engine search/stop/UCI-I/O code land under it
- [ ] #2 Each of the three failure modes (intermittent search/UCI deadlock; illegal PV moves; EOF null-move abort) is reproduced and root-caused, with documented evidence (repro conditions, captured state at the failure, offending positions/PVs, and the relevant code paths)
- [ ] #3 The investigation determines whether the failures are independent or share a common root cause, and records any coupling with TASK-32 (time allocation) so overlapping fixes are not duplicated
- [ ] #4 One or more fresh, well-scoped implementation tickets are created that spec the fix for each defect (or root cause), each with its own acceptance criteria so it can be implemented and reviewed independently; those tickets carry forward the original fix-level requirements (no hang under repeated self-play, only-legal PV moves, legal best-so-far move on stdin EOF, and regression coverage of the stop/abort and EOF paths)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce each of the three failure modes against the real FastChess build and scripted UCI input; capture concrete evidence (offending PV, bestmove 0000, thread samples at the hang).
2. Root-cause each defect against the search/stop/UCI-I/O code paths (engine/src/engine.rs, search.rs, pv_table.rs, info.rs); determine independent vs shared cause and coupling with TASK-32.
3. Record investigation findings as a durable backlog doc plus task notes (no engine code changes land under TASK-34).
4. Create fresh, independently-implementable tickets speccing the fix for each root cause, each carrying forward the original fix-level requirements and regression-coverage expectations.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Investigation complete; no engine code changed under this ticket (working tree touches only backlog/). Findings recorded in doc-2 (backlog/docs/doc-2). All three failure modes reproduced and root-caused:
- Defect 3 (EOF null move): deterministic repro 'printf uci/isready/go depth 25 | seaborg -u' -> bestmove 0000 from startpos. Root cause: EOF cancels the search before a depth completes; iterative_deepening records no result -> Cancelled(None) -> format_search_outcome emits 0000.
- Defect 2 (illegal PV): FastChess depth=4 self-play flags 'Illegal PV move - move c5f8' for 'pv d7f8 g6a6 f8g6 c5f8' (score mate -2). Best move (first ply) is legal; deep PV plies corrupt. Root cause: triangular PVTable updated on fail-high/cutoff nodes (search.rs Step 22) splices stale sibling rows via copy_within; mate/leaf handling compounds it.
- Defect 1 (completion deadlock): reproduced under debug-build self-play (concurrency>=8); all slots freeze, engines idle at ~0% CPU, no bestmove, no panic. Thread samples at the hang show the driver parked in crossbeam select! on the active-search branch while the search worker thread has already exited (only main+reader threads remain) -> lost channel-disconnect wakeup; finish_search never runs.
Coupling: Defects 1 and 2 are independent of each other and TASK-32. Defect 3 shares TASK-32's root cause (no guaranteed legal move before an abort; differ only in trigger: time budget vs EOF).
Fresh tickets: TASK-35 (Defect 1), TASK-36 (Defect 2), TASK-37 (Defect 3, depends on/coupled to TASK-32; coupling also noted on TASK-32).
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
<!-- COMMENTS:END -->
