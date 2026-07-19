---
id: TASK-63
title: Make movestogo and movetime allocation a deliberate policy
status: To Do
assignee: []
created_date: '2026-07-19 13:17'
labels:
  - engine
  - time
  - uci
dependencies:
  - TASK-42
priority: medium
type: bug
ordinal: 62000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The allocation policy in engine/src/time.rs was designed around sudden-death and increment controls. Periodic (movestogo) controls were never given a deliberate policy and fall out of the code by accident, and the movetime path bypasses the safety margin entirely. This was an omission from the TASK-42 definition, identified during its review.

## What a movestogo control means

'movestogo n' says: play n more moves and another time grant arrives. In standard implementations unspent time accumulates across the boundary. The goal is therefore to reach the boundary having spent the period's budget with a small cushion intact, since that cushion carries rather than being lost. Current behaviour departs from this in four ways.

## Defects

(a) 'movestogo 0' is read as one move left. est_remaining_moves is 'Some(n) => n.max(1)', so 0 becomes 1 and the engine commits three quarters of its clock to a single move. UCI does not define 0, and GUIs emit it loosely, often meaning no movestogo or the boundary itself. The existing test explicit_moves_to_go_controls_allocation_and_zero_is_safe asserts 7478ms of a 10000ms clock and calls it safe. It is forfeit-safe, because three quarters of a shrinking clock decays geometrically and never reaches zero, but it is strategically wrong. A 0 means unknown and should fall back to the AVERAGE_GAME_LENGTH heuristic, not to maximum aggression. This defect predates TASK-42.

(b) The reserve TASK-42 introduces is dimensionally wrong here. RESERVE_INCREMENT_MOVES * inc is calibrated on the premise that the increment funds the steady state. Under a periodic control the grant funds it, not the increment. The reserve also binds hardest exactly where it is least needed: near a boundary the clock is small, so allocation falls into the below-reserve branch, yet new time is imminent so a reserve matters least. The behaviour is inverted. Either exempt movestogo from the reserve or scale the reserve to boundary distance so it relaxes on approach. Concretely, at clock 1000, inc 100, movestogo 1 the allotment falls from 728ms before TASK-42 to 97ms after it.

(c) No boundary cushion. Dividing by exactly n plans to arrive at the boundary with nothing, which ignores search overshoot. Divide by n plus a small constant.

(d) MAX_CLOCK_SHARE_DIVISOR is doing policy work it was not designed for. It is a backstop against pathological input, but at movestogo 1 it is the allocation policy. The policy should produce a sensible number the cap never has to touch.

(e) movetime bypasses MOVE_OVERHEAD. engine/src/engine.rs maps TimingMode::MoveTime(t) straight to Duration::from_millis(t), while the Timed path holds MOVE_OVERHEAD back for the bestmove round trip. This is inconsistent and is a forfeit risk under a GUI that enforces movetime strictly.

## Out of scope

Sudden-death decay rate. Past move 20 MINIMUM_REMAINING_MOVES pins the estimate at 20, so allocation is clock/20 every move and the engine reaches the endgame with very little. Spending down is correct in sudden death and the geometric shape never flags, so this is a design question about the spend curve rather than a defect. Track separately if it is worth revisiting.

Distinct from TASK-40, which concerns how well a single move spends its allotment. This ticket concerns how much is allotted under periodic and fixed-time controls.

Do not regress TASK-7 overflow safety, TASK-32 guaranteed legal move under a zero budget, TASK-38 proportional opening allocation, or the TASK-42 increment reserve.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 movestogo 0 is treated as an unknown horizon and falls back to the sudden-death remaining-move heuristic rather than to a single remaining move, with a test asserting it no longer commits the share-cap maximum
- [ ] #2 The increment reserve does not bind harder as a control boundary approaches; a test covers a small clock near a boundary and shows the allotment is governed by the moves remaining in the period rather than by the increment reserve
- [ ] #3 Allocation under movestogo divides by the remaining moves plus a cushion, so a simulated period ending at the boundary arrives with time still on the clock, asserted by a test over a representative periodic control
- [ ] #4 MAX_CLOCK_SHARE_DIVISOR is not the binding constraint for any well-formed movestogo input, evidenced by a test sweeping movestogo values and asserting the policy result is already within the cap
- [ ] #5 TimingMode::MoveTime deducts MOVE_OVERHEAD consistently with the Timed path, with a test covering a movetime smaller than the overhead that still yields a non-negative searchable budget
- [ ] #6 TASK-7 overflow safety, TASK-32 guaranteed-legal-move behavior, TASK-38 proportional opening allocation and the TASK-42 increment reserve all still hold, evidenced by their existing regression tests passing unmodified
<!-- AC:END -->
