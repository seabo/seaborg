---
id: TASK-36
title: Fix illegal moves in the reported principal variation (info ... pv ...)
status: To Do
assignee: []
created_date: '2026-07-18 01:21'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: high
type: bug
ordinal: 41000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
seaborg reports principal variations over UCI (info ... pv ...) that contain illegal moves. Reproduced authoritatively in TASK-34 with FastChess self-play at depth=4: 'Warning; Illegal PV move - move c5f8' for the line 'info depth 4 multipv 1 score mate -2 ... pv d7f8 g6a6 f8g6 c5f8'. The reported best move (first PV ply) is legal — games complete with correct results — so this is a PV-reporting defect, not a move-selection defect; the corruption is on deeper PV plies and shows up on mate-scored / shallow lines emitted during play.

Root cause (from TASK-34, doc-2): the PV shown over UCI is rebuilt from the triangular PVTable (engine/src/pv_table.rs) via emit_progress -> pvt.pv() (search.rs:931-941). The table is updated on every alpha-raise, including fail-high/beta-cutoff nodes: search.rs:671-698 calls pvt.copy_to(depth, mov) in the value>=beta branch before break 'move_loop. copy_to/update_internal splice the child row into the parent via copy_within, but on a cutoff (and around mate/leaf handling via pv_leaf_at) the child row can still hold moves from a different sibling subtree, which get copied up — so the reconstructed line does not chain legally beyond the first move.

Scope: ensure every move in the reported PV is legal in the position reached by playing the preceding PV moves, without changing which move the engine selects/plays. Fix the PV reconstruction so stale sibling entries and mate/leaf handling cannot splice illegal continuations (e.g. only update the PV on exact/alpha-raising PV nodes, correctly clear/propagate child rows, or reconstruct the PV from validated data). This defect is independent of the completion-deadlock and EOF defects and of TASK-32.

Relevant code: engine/src/pv_table.rs (copy_to/update_internal/pv_leaf_at/pv), engine/src/search.rs (Step 22 PV update on cutoffs, mate/stalemate leaf handling), engine/src/info.rs (format_search_event). See backlog doc-2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every move in every reported 'info ... pv ...' line is legal in the position reached after playing the preceding PV moves, including on mate/stalemate-scored and beta-cutoff lines
- [ ] #2 Validated with FastChess (or cutechess) seaborg self-play at fixed depth: zero 'Illegal PV move' warnings across a multi-game match
- [ ] #3 A regression test drives the search on positions that previously produced illegal PVs (including the mate line d7f8 g6a6 f8g6 c5f8) and asserts the full reported PV is legal by playing it out
- [ ] #4 The engine's selected/played best move is unchanged by the fix; existing search-correctness tests (e.g. gives_correct_answers) still pass
<!-- AC:END -->
