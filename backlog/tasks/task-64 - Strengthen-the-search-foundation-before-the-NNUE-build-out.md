---
id: TASK-64
title: Strengthen the search foundation before the NNUE build-out
status: To Do
assignee: []
created_date: '2026-07-19 13:30'
labels:
  - search
  - nnue
  - architecture
  - strength
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
  - engine/src/eval.rs
priority: high
type: feature
ordinal: 63000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella for the search work that must land before an NNUE training run begins.

Rationale. A network trained by distilling this engine's own search inherits whatever that search can see. Training on top of a search that lacks late move reductions, null move pruning, aspiration windows and a working history heuristic caps the quality of every label in the dataset, and the resulting checkpoint bakes that cap in. The cost of doing this work first is one-off; the cost of not doing it is paid again at every subsequent training generation.

Current state, as audited on 2026-07-19 at commit 8185470. The alpha-beta skeleton is sound: PVS with null-window scout and re-search, staged lazy move generation behind the ordering::Loader trait, promotion-aware SEE, mate-distance pruning, a lock-free transposition table, and careful abort, mate-score and history-sensitivity semantics backed by an extensive test suite. What is absent is essentially all selectivity. There are no reductions, no extensions of any kind (not even a check extension), no null move pruning, no aspiration windows, and the history heuristic is allocated and read but never written.

Structural blocker. Ply-from-root is derived, not tracked: search.rs:646 computes `let draft = self.search_depth - depth`, and PVTable::clear_at/copy_to compute their row the same way (pv_table.rs:55-57, :70-72). This is correct only while depth decreases by exactly one per recursion. Any extension makes depth exceed search_depth and underflows that u8 subtraction; any reduction makes sibling subtrees at the same ply carry different depth, so killers and PV rows are indexed by the wrong ply. Late move reductions, extensions, singular extensions and a quiescence ply cap all violate the assumption. The ply and search-stack refactor is therefore a hard prerequisite for a large part of this programme, not a cleanup.

Evaluation caveat. Search selectivity and evaluation quality are not independent here. `Search::evaluate` (search.rs:1096) returns material only, with knight and bishop both valued at 300 and no piece-square, mobility, king safety, pawn structure or game-phase terms. Several pruning techniques in this programme decide what to discard by comparing a static evaluation against a margin, and a material-only evaluation makes those decisions close to arbitrary. Expect smaller gains from the margin-based prunings until the evaluation work lands, and treat that as a reason to sequence rather than to skip.

Measurement discipline. Existing tickets in this area deliberately land one at a time so that strength changes stay attributable (see the TASK-50 to TASK-51 to TASK-52 chain). That discipline applies across this programme. Dependencies recorded on the child tasks capture mechanical prerequisites only; they are not a claim that unrelated children may be measured concurrently. Every child that can move playing strength is measured with the TASK-27 regression script against the immediately preceding commit, with results recorded in its implementation notes.

Scope. This task tracks the programme and is complete when its children are. It carries no implementation of its own.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every child task is Done or explicitly closed with a recorded decision not to pursue it
- [ ] #2 A closing summary records the measured strength delta of the programme as a whole against commit 8185470, using the TASK-27 script
- [ ] #3 The evaluation-quality caveat is revisited before the NNUE training run begins, and the decision to proceed is recorded
<!-- AC:END -->
