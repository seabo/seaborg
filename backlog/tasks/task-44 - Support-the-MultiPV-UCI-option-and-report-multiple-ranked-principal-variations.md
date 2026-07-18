---
id: TASK-44
title: Support the MultiPV UCI option and report multiple ranked principal variations
status: To Do
assignee: []
created_date: '2026-07-18 14:02'
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

An enabling property of the current search is worth recording, because it is what makes this tractable and could silently stop being true: the root is searched with beta = INF_P and mate scores are bounded well below it, so the beta-cutoff branch is unreachable at the root and every root move is already searched with a full window and an exact score. Root move ordering and per-move exact values are therefore already available; what is missing is retaining the top K of them, keeping a separate PV per line, and reporting each with its own multipv index.

Interaction to be aware of, not a blocking dependency: TASK-43 extends a single reported PV with validated transposition-table moves. Both tasks change how PV lines are stored and reported, so whichever lands second should apply its behaviour per line rather than to a single global PV. Neither blocks the other.

Sequencing note: this is an analysis convenience and sits behind the outstanding search fundamentals (reductions, extensions and late move reduction are still TODO at Steps 16 and 17 of engine/src/search.rs). It is filed as Low priority deliberately.

Relevant code: engine/src/info.rs (format_search_event), engine/src/uci.rs (option advertisement and setoption parsing), engine/src/search.rs (root move loop, emit_progress), engine/src/pv_table.rs. Background: TASK-36, TASK-43, backlog doc-1.
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
