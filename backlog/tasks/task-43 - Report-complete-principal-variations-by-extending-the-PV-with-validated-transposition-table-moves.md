---
id: TASK-43
title: >-
  Report complete principal variations by extending the PV with validated
  transposition-table moves
status: To Do
assignee: []
created_date: '2026-07-18 13:59'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: low
type: enhancement
ordinal: 44000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-36 made every reported "info ... pv ..." line legal by publishing only plies that came from an exact PV-node alpha raise, with the triangular PVTable row cleared on entry to every node. That is correct, but deliberately conservative: a fail-high node has no exact continuation to publish, and the mating move at the end of a forced line usually arrives as a fail-high. The result is that mate-scored lines are reported truncated. Measured on the TASK-36 diff, a position scored "mate 3" reports "pv c7c6 a6a5" where the informative line is five plies, and all PV changes introduced by TASK-36 were on mate-scored lines (non-mate PVs were byte-identical to the previous behaviour).

This is a reporting and diagnostics defect, not a strength defect. It affects what a UCI GUI displays and how legible the search is when debugging; it does not change which move the engine plays.

The conventional remedy is a hybrid PV: keep the triangular table as the trusted exact prefix, then extend past its last ply by walking the transposition table, playing each TT move only after confirming it is legal in the position reached, and stopping on a hash miss, an illegal or stale TT move, a repetition, or a sensible length cap. This recovers full mate lines and depth-length PVs without reintroducing the stale-sibling splice that TASK-36 fixed, because every extended ply is validated against a real position rather than copied from a table row.

The verification harness already exists and should be reused rather than rebuilt: engine/src/search.rs has reported_principal_variations_are_legal, which replays every emitted PV from the root and asserts legality, and the FastChess self-play A/B at fixed depth is the end-to-end check. Relevant code: engine/src/pv_table.rs, engine/src/search.rs (emit_progress), engine/src/tt.rs, engine/src/info.rs. Background: TASK-36 and backlog doc-2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A search that reports a mate score reports the full mating line: the PV length in plies equals the mate distance whenever that line is recoverable from the triangular table plus the transposition table
- [ ] #2 Every move in every reported PV is still legal in the position reached after playing the preceding PV moves, on mate-scored, beta-cutoff and ordinary lines; the existing reported_principal_variations_are_legal regression test still passes unmodified
- [ ] #3 PV extension terminates safely and never emits a wrong or unbounded line: a transposition-table miss, a stale or illegal TT move, a repetition, and a length cap each stop the extension, and a test covers each of these stop conditions
- [ ] #4 FastChess (or cutechess) seaborg self-play at fixed depth produces zero "Illegal PV move" warnings across a multi-game match
- [ ] #5 The engine selected/played best move is unchanged and search node counts are identical to the pre-change build for the same position and depth, proving PV extension happens only at reporting time and does not perturb the search
- [ ] #6 A test asserts reported PV length, not just legality, for at least one known forced-mate position and one non-mate position searched to a fixed depth
<!-- AC:END -->
