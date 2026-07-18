---
id: TASK-37
title: Return a legal best-so-far move on stdin EOF instead of bestmove 0000
status: To Do
assignee: []
created_date: '2026-07-18 01:21'
labels:
  - engine
  - search
  - uci
dependencies:
  - TASK-32
priority: high
type: bug
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When stdin is closed while a search is running (e.g. a fire-and-forget 'uci/isready/go' pipe with no explicit stop/quit), seaborg emits 'bestmove 0000' instead of a legal move. Reproduced deterministically in TASK-34: printf 'uci\nisready\ngo depth 25\n' | seaborg -u  ->  'bestmove 0000' from the startpos (20 legal moves).

Root cause (from TASK-34, doc-2): on EOF the reader sends Input::Closed and the driver cancels the running search and finishes it (engine/src/engine.rs:53-60). iterative_deepening only records a SearchResult for an iteration that completes while not stopping (search.rs:447-457); if the cancel lands before even depth 1 completes, the outcome is Cancelled(None) and format_search_outcome maps None to 'bestmove 0000' (info.rs:34-38). The engine never guarantees a chosen legal root move before the abort.

COUPLING WITH TASK-32 (must coordinate to avoid duplicated fixes): this is the SAME underlying defect as TASK-32 (illegal null move at fast time controls) — no guaranteed legal move before an abort. They differ only in the abort trigger (TASK-32: zero/near-zero time budget; this ticket: stdin EOF/cancel). The shared guarantee ('always choose a legal move before any abort takes effect; return the legal best-so-far') should be implemented once. Recommended: implement the core guarantee under TASK-32 and have this ticket add the EOF trigger + regression coverage on top; or, if this lands first, implement the shared guarantee here and narrow TASK-32 to the time-budget path. Either way, do not implement two divergent fixes for the same root cause.

Scope: on stdin EOF while a search runs in a non-terminal position, the engine must emit a legal move (its best-so-far, or a guaranteed first legal root move) and never 'bestmove 0000'. In a genuinely terminal position (no legal moves) 'bestmove 0000' remains correct. Independent of the completion-deadlock and illegal-PV defects.

Relevant code: engine/src/engine.rs (Input::Closed handling, stop_search/finish_search), engine/src/search.rs (iterative_deepening best-so-far / minimum one-ply guarantee, cancellation), engine/src/info.rs (format_search_outcome). See backlog doc-2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On stdin EOF while searching a non-terminal position, the engine emits a legal move (best-so-far or guaranteed first legal root move) and never 'bestmove 0000'; a terminal position with no legal moves still yields 'bestmove 0000'
- [ ] #2 A guaranteed-minimum search selects at least one legal root move before any EOF/cancel abort can take effect
- [ ] #3 The shared 'legal move before any abort' guarantee is implemented once and coordinated with TASK-32 (no duplicate/divergent fix for the same root cause); the coupling and division of work is recorded on both tickets
- [ ] #4 Regression tests cover the stdin-EOF/stop-abort path returning a legal move (e.g. the fire-and-forget 'uci/isready/go' EOF scenario) and the terminal-position case
<!-- AC:END -->
