---
id: TASK-44
title: Support the MultiPV UCI option and report multiple ranked principal variations
status: To Do
assignee: []
created_date: '2026-07-18 14:02'
updated_date: '2026-07-18 14:05'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: low
type: enhancement
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
MultiPV is currently not implemented. The "multipv 1" field in the emitted info line is a hardcoded string literal in the format string at engine/src/info.rs, not a real value: there is no MultiPV option in the UCI handshake, no setoption parsing for it, and no multi-line search. Responding to "uci" advertises only "option name Hash type spin default 16 min 1 max 1024". Emitting a constant "multipv 1" is harmless and spec-acceptable for single-PV mode, so this is a missing feature rather than a defect.

MultiPV matters for analysis rather than playing strength: it lets a GUI or the local browser UI show the top K candidate moves with their scores and lines, which is also the most useful view when debugging move ordering and evaluation.

Cost note, corrected. An earlier version of this task claimed the root already produces an exact score for every root move, making MultiPV nearly free. That is wrong and should not be relied on. The root uses principal variation search: at Step 19 of engine/src/search.rs only the first root move is searched with a full window, and moves 2 and later are searched with a null window first and re-searched fully at Step 20 only when they raise alpha. A root move scoring at or below alpha therefore carries an upper bound, not an exact score. What is true, and is a weaker property, is that beta at the root is INF_P and mate scores are bounded well below it, so the beta-cutoff branch is unreachable and no root move is ever skipped by a cutoff. Every root move is visited; not every root move is exactly scored.

The implementation is therefore the conventional one: run K passes over the root, each excluding the already-selected best moves and each searching with a full window, retaining a separate PV and exact score per line.

Sequencing, with the rework risk assessed rather than assumed. This work is a root-level construct and is largely stable under the search improvements that are still outstanding: late move reduction, other reductions, and extensions are TODO at Steps 16 and 17 but all apply below the root and do not disturb a root exclusion loop. The one genuine interaction is aspiration windows, which narrow the root window and would require per-line alpha and beta bookkeeping; that is an adjustment to the loop rather than a rewrite. The cost of this task is therefore roughly the same before or after the pending search work, and it is filed as Low priority because it buys analysis convenience rather than playing strength, not because deferring it avoids rework.

Interaction to be aware of, not a blocking dependency: TASK-43 extends a single reported PV with validated transposition-table moves. Both tasks change how PV lines are stored and reported, so whichever lands second should apply its behaviour per line rather than to a single global PV. Neither blocks the other.

Relevant code: engine/src/info.rs (format_search_event), engine/src/uci.rs (option advertisement and setoption parsing), engine/src/search.rs (root move loop at Steps 19 and 20, emit_progress), engine/src/pv_table.rs. Background: TASK-36, TASK-43, backlog doc-1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The engine advertises a MultiPV spin option in its response to "uci" with a default of 1 and a documented maximum, and "setoption name MultiPV value N" is parsed and applied
- [ ] #2 With MultiPV set to N > 1, each completed search iteration emits N info lines carrying distinct multipv indices 1..N, ordered best first, each with its own score and principal variation
- [ ] #3 The multipv field reflects the actual line index rather than a constant, and with MultiPV set to 1 the emitted output is unchanged from current behaviour
- [ ] #4 Every move of every reported line is legal in the position reached after playing the preceding moves of that same line; the reported_principal_variations_are_legal regression test is extended to cover all lines when MultiPV > 1
- [ ] #5 The move played is the multipv 1 move, and with MultiPV set to 1 the selected best move and search node counts are identical to the pre-change build for the same position and depth
- [ ] #6 Requesting more lines than there are legal moves reports only the available lines without error, and MultiPV is accepted in a position with a single legal move
- [ ] #7 FastChess (or cutechess) seaborg self-play at fixed depth produces zero "Illegal PV move" warnings across a multi-game match with MultiPV at its default
<!-- AC:END -->
