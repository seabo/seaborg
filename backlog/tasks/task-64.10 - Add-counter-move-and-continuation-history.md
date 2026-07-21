---
id: TASK-64.10
title: Add counter-move and continuation history
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 03:10'
labels:
  - search
  - move-ordering
dependencies:
  - TASK-64.1
  - TASK-64.2
  - TASK-64.3
  - TASK-64.17
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
Quiet move ordering currently combines a two-slot per-ply killer stage with one side-specific from-to butterfly history table. Add counter-move and continuation history so ordering can condition a quiet move on the moves that preceded it rather than only on its origin and destination.

Current state. HistoryTable holds one 64x64 from-to table per side. There is no counter-move table and no continuation history. The staged order is HashTable, QueenPromotions, GoodCaptures, EqualCaptures, Killers, Quiet, BadCaptures, Underpromotions. TASK-64.3 repairs the killer table into a small recency cache of same-ply refutations; this task must determine empirically how that cache should coexist with stronger contextual evidence rather than assuming every heuristic deserves a permanent independent stage.

Continuation history is a major remaining move-ordering opportunity. A global from-to table cannot distinguish a move that is generally useful from one that is specifically a strong reply to the preceding position. Maintain continuation evidence for at least one and two plies back; consider additional distances only with a recorded rationale and acceptable memory/cache behavior.

A counter-move table is the one-ply special case that retains one candidate reply to the previous move. A dedicated counter stage after killers is a reasonable initial implementation, but it is a hypothesis rather than a required final architecture. Compare it against folding counter and killer candidates into a combined contextual quiet ranking. Also measure whether equal captures should remain ahead of killers. Prefer the simplest ordering that wins on fixed-depth node count, throughput and strength.

Use the per-ply search stack to obtain preceding moves. Share the bounded bonus, malus and aging scheme established for plain history rather than introducing independent unbounded counters. New candidates or stages must participate in hash, killer, counter and quiet duplicate suppression and every externally stored move must be validated before unsafe execution.

This depends on TASK-64.1, TASK-64.2, TASK-64.3 and TASK-64.17. Coordinate measurement with TASK-64.3: once contextual history is active, run an ablation with killers disabled, one slot and two slots. Retaining, combining or deleting killers are all acceptable outcomes when supported by results.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A counter-move is tracked by previous move and participates in ordering with complete duplicate suppression; a dedicated stage after killers may be the initial implementation but is not mandated as the final architecture
- [ ] #2 Continuation history is maintained for at least one and two plies back and contributes to quiet move ordering
- [ ] #3 The implemented continuation distances, indexing scheme, memory footprint and expected per-worker ownership are recorded with rationale
- [ ] #4 Bonus, malus and aging use the bounded scheme established for plain history rather than separate unbounded or exposure-based counters
- [ ] #5 Tests show that contextual evidence can order a reply ahead of a move with higher plain history and cover duplicate suppression against hash, killer, counter and ordinary quiet candidates
- [ ] #6 Externally stored killer and counter candidates are legality-validated before unsafe move execution
- [ ] #7 Fixed-depth node counts and search throughput compare a dedicated killer/counter stage with a combined contextual quiet-ranking design, and compare equal captures before versus after refutation candidates
- [ ] #8 After contextual history is active, an ablation compares killers disabled, one slot and two slots; the recorded decision may retain, combine or remove the killer heuristic
- [ ] #9 Representative fixed-depth node counts improve without an unacceptable throughput regression, with figures recorded in implementation notes
- [ ] #10 The selected design is measured with the TASK-27 strength-regression script and results are recorded in implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Shared bounded update. Extract the plain-history gravity update (bonus/malus/aging, clamp to +/-HISTORY_MAX) into one primitive reused by every contextual table so counter/continuation evidence uses the same bounded scheme, not independent unbounded counters.
2. Counter-move table. Key one quiet reply by the previous move's (moving piece, destination). Per-worker field on Search, reset each search alongside history/killers. Read is legality-validated with Position::valid_move before it can be executed.
3. Continuation history. Two gravity tables for 1 and 2 plies back, indexed [prev piece-to context][current piece-to] as i32, per-worker, reset each search. Record distances, indexing, footprint and per-worker ownership in notes.
4. Stack context. Store the moving piece per ply in StackEntry when the move is set (before make), so the (piece,to) context of the 1- and 2-ply-back moves is read directly from the stack (null/none handled).
5. Ordering integration (primary = combined contextual quiet ranking). score_quiets sums plain history + cont-hist(1) + cont-hist(2) + counter bonus. On a quiet beta cutoff: bonus the cutoff move and malus the earlier failed quiets across plain history and both continuation distances, and store the counter move. Killers remain a stage; duplicate suppression must cover hash/killer/counter/quiet.
6. AC#7 A/B via compile-time knobs (KILLER_SLOTS precedent): counter as a dedicated-stage-equivalent dominant bonus vs combined moderate bonus; equal captures before vs after refutation candidates.
7. Tests. Contextual evidence orders a reply ahead of a higher plain-history move; duplicate suppression vs hash/killer/counter/quiet; counter legality validation; bounded update shared.
8. Measurements. Extend the ablation example for node counts + throughput across the AC#7 variants and the killer ablation (AC#8) with continuation history active; run the TASK-27 strength SPRT for the selected design (AC#9/#10). Record all figures in implementation notes.
<!-- SECTION:PLAN:END -->
