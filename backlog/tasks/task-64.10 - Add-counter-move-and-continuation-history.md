---
id: TASK-64.10
title: Add counter-move and continuation history
status: To Do
assignee: []
created_date: '2026-07-19 13:32'
labels:
  - search
  - move-ordering
dependencies:
  - TASK-64.1
  - TASK-64.2
references:
  - engine/src/history.rs
  - engine/src/ordering.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 73000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Quiet move ordering uses a single butterfly from-to table and nothing else. Add the counter-move heuristic and continuation history, which condition the score of a quiet move on the moves that preceded it rather than on the moving piece alone.

Current state. `HistoryTable` (history.rs:79-82) holds one 64x64 from-to table per side and nothing more. There is no counter-move table, no continuation history at any distance, and the ordering phases (ordering.rs:242-266) run HashTable, QueenPromotions, GoodCaptures, EqualCaptures, Killers, Quiet, BadCaptures, Underpromotions with no stage between Killers and Quiet where a counter-move would conventionally sit.

Continuation history is the single largest remaining move-ordering gain available once plain history is working. A from-to table cannot distinguish a quiet move that is good in general from one that is good specifically as a reply to the opponent's last move, and most quiet moves that matter are of the second kind.

The counter-move heuristic is the one-ply special case: index the previous move to a single refutation move, and try it after the killers. Continuation history generalises it to a score keyed on the previous move at one, two and four plies back. Which distances to implement is a scope decision to settle and record; one and two ply are the usual minimum.

This depends on the ply and search-stack refactor, because continuation history requires reading the move played at ply minus N, which is exactly the per-ply state that refactor introduces and which has nowhere to live today. It depends on history activation because the bonus, malus and aging scheme established there should be shared rather than reinvented per table.

Adding a counter-move stage means a new Phase variant and a corresponding Loader method. The ordering module dedups later phases against earlier ones explicitly (`dedup_segments`, ordering.rs:546-554, and the re-checks in KillerIter and QuietsIter), and a new stage must participate in that scheme rather than yield duplicates.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A counter-move is tracked per previous move and yielded as an ordering stage after killers, participating in the existing duplicate-suppression scheme
- [ ] #2 Continuation history is maintained for at least one and two plies back and contributes to quiet move scores
- [ ] #3 The set of continuation distances implemented is recorded with rationale
- [ ] #4 Bonus, malus and aging follow the scheme established for plain history rather than a separate ad hoc scheme
- [ ] #5 A test asserts that a quiet move good only as a reply to a specific previous move is ordered ahead of a quiet move with a higher plain history score
- [ ] #6 Node counts at fixed depth are reduced on a representative position set, with figures recorded in the implementation notes
- [ ] #7 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->
