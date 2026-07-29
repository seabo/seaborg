---
id: TASK-94
title: 'Razoring fires in PV and in-check nodes, contrary to its own guards'
status: To Do
assignee: []
created_date: '2026-07-29 18:38'
labels:
  - search
  - pruning
dependencies:
  - TASK-64.4
references:
  - engine/src/search.rs
priority: medium
type: bug
ordinal: 163000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Razoring omits the non-PV and not-in-check guards that its documented design and its sibling reverse-futility-pruning (RFP) both require, opening an unsound early-return on PV and in-check nodes.

The Step-7 comment (engine/src/search.rs ~2384-2386) states that razoring and RFP "share the same guards: non-PV node, not in check ... and a centipawn window bound." RFP (~2402-2410) enforces all three (`!Node::pv()`, `!self.pos.in_check()`, `beta.is_cp()`). But the razoring call site (~2390) delegates entirely to `should_razor` (~178-189), which checks only `depth <= 6 && alpha.is_cp() && eval + margin < alpha` -- it carries NEITHER `!Node::pv()` NOR `!self.pos.in_check()`. Null-move (~2441) and futility (~2420) are likewise guarded; razoring is the outlier.

Consequences:
- At a PV node (including the root under an aspiration window, where `alpha.is_cp()` holds), when `eval + margin < alpha` the node runs `quiesce(alpha-1, alpha)` and, on a result below alpha, returns `Some(value)` WITHOUT searching any move. That violates the PVS invariant that a PV node is searched to an exact score, and an unsound shallow fail-low on the principal variation can corrupt best_move/score propagation to the parent.
- At an in-check node, `eval` does not reflect being in check, so a tactically forced position can be pruned on a shallow qsearch verdict.

Interaction with TASK-64.4 (correct the razoring depth margin): today the squared margin (678cp at d1, 1434cp at d2, ...) makes razoring near-inert, so these missing guards rarely bite and the current Elo cost is small. Once TASK-64.4 corrects the margin so razoring fires routinely, the missing guards become a live soundness hole on PV/in-check nodes. This guard fix should therefore land with or before the margin correction; hence the dependency. The fix is essentially free.

The fix is to gate the razoring block on `!Node::pv() && !self.pos.in_check()` (and the same forward-pruning/rfp enable predicate other techniques use, for test-hook parity), mirroring the adjacent RFP block. Found via an automated correctness audit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Razoring is gated on non-PV and not-in-check, matching RFP and the Step-7 comment
- [ ] #2 Razoring respects the same forward-pruning enable predicate the neighbouring techniques use, so tests can toggle it consistently
- [ ] #3 A test asserts razoring does not return early at a PV node or while in check even when eval + margin < alpha
- [ ] #4 Change measured with the TASK-27 strength-regression script; results recorded in the implementation notes, including the outcome if the guard-only change measures neutral at the current margin
<!-- AC:END -->
