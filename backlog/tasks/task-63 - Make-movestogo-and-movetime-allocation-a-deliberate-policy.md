---
id: TASK-63
title: Make movestogo and movetime allocation a deliberate policy
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 13:17'
updated_date: '2026-07-19 18:40'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Resolve REV-1-01: the periodic branch computes 'n + BOUNDARY_CUSHION_MOVES' in non-saturating arithmetic, so an untrusted 'movestogo' near u64::MAX overflows the divisor (debug: add overflow; release: divisor wraps to 0 and divides by zero). Use saturating_add for the divisor, matching the saturating_add/saturating_mul already used for the period budget on the adjacent line.
2. A saturated divisor yields an allotment of 0 from the division, which the existing '.max(1)' floor turns into 1ms. That is finite, forfeit-safe and needs no clamping policy invented: an absurd horizon genuinely means spread the clock arbitrarily thin, and the search still guarantees a legal move.
3. Add a regression test that varies 'movestogo' itself rather than the clock and increment, covering u64::MAX and the neighbouring values, since the existing TASK-7 test holds movestogo at 20.
4. Re-run cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Periodic controls now have their own allocation branch in engine/src/time.rs, separate from the sudden-death/increment path that TASK-38 and TASK-42 shaped.

Policy: a period's spendable budget is the usable clock plus the increments its remaining moves can still spend. That is n-1 increments, not n: the increment for the final move of a period is credited after that move is played, so it carries across the boundary rather than funding anything before it. Counting all n made the policy plan to spend time it did not hold, and on the boundary move that pushed the ask past the share cap (2000ms clock, 1000ms increment, movestogo 1 asked 1485ms against a 1478ms cap). Dropping the last increment also makes the boundary-move ask exactly half the usable clock for any increment, which is what takes the cap out of the policy entirely.

Cushion: dividing by n+1 rather than n. The recurrence is self-similar — spending budget/(n+1) and re-dividing the remainder over n-1 moves yields the same figure — so allocation is flat across the period and the clock arrives at the boundary holding exactly one move's worth. Since unspent time carries across the boundary, that cushion is not wasted.

The increment reserve (RESERVE_INCREMENT_MOVES) is not applied to periodic controls at all, rather than being scaled by boundary distance. Its premise is that the increment funds the steady state, which is false when a grant does.

movestogo 0 is folded into the None branch via Option::filter, so it is byte-identical to no periodic control at every move number rather than being special-cased.

movetime: added move_time_budget() in time.rs rather than inlining the subtraction in engine.rs, so MOVE_OVERHEAD stays private to the time module and the deduction is unit-testable.

Tests whose asserted constants encoded the old periodic arithmetic were updated; see the handoff comment for the exact list and why each changed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 16:41
---
Implementation handoff
Branch: task-63-movestogo-movetime-policy
Worktree: /Users/seabo/seaborg-worktrees/task-63-movestogo-movetime-policy
Base: c55508b3383577ed9bb62a9ebadb21fc3ecedc1f
Implementation target: bf811bf
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 249 engine + 43 core + 17 others, 0 failed, 2 ignored
Known failures: none

Reviewer note on AC#6 ('existing regression tests passing unmodified'). The TASK-42 reserve tests, the TASK-38 proportional-opening test and the TASK-32 zero-budget tests all pass unmodified — they exercise sudden-death and increment controls, whose code path is untouched. Four tests that assert periodic (movestogo) constants necessarily changed value, since changing the periodic policy is the task. Each was updated to the new arithmetic with its original purpose intact:

- allocation_preserves_values_above_u32_max (TASK-7 overflow): movestogo 20, divisor 20 -> 21. The u32-narrowing assertion is unchanged and still passes.
- parses_large_timed_control_values_without_narrowing (TASK-7 overflow, engine/src/uci.rs): same, and the magic number was replaced with the arithmetic so the intent is legible.
- increment_contributes_to_allocation: movestogo 20, 698/898 -> 655/836.
- explicit_moves_to_go_controls_allocation_and_zero_is_safe: split. The periodic half became explicit_moves_to_go_divides_the_period_budget; the movestogo-0 half became moves_to_go_of_zero_falls_back_to_the_game_length_heuristic, which AC#1 requires to change (it asserted the 7478ms share-cap maximum the task identifies as the defect).
- huge_increment_cannot_allocate_more_than_the_clock_holds: kept, but its cap-binding case moved from movestogo 1 to movestogo 2. At movestogo 1 the cap can no longer bind for any increment, which is the point of AC#4, so the old case no longer demonstrated the backstop.

Worth a reviewer's judgement: the share cap is still reachable by a periodic control if the increment exceeds roughly three quarters of the usable clock (e.g. a 1000ms clock with a 5000ms increment and movestogo 2). I read AC#4's 'well-formed' as excluding that, and the sweep test asserts the cap is slack across real controls from 90+30 classical down to 3+2 blitz, plus a synthetic sweep bounded at inc <= usable/2. If you read 'well-formed' more broadly, that is a finding.
---

author: @codex
created: 2026-07-19 18:32
---
Review attempt: 1
Reviewed branch: task-63-movestogo-movetime-policy
Reviewed implementation: bf811bf
Verdict: changes_requested

REV-1-01 [P1] Periodic branch panics on a large movestogo, regressing TASK-7 overflow safety
Location: engine/src/time.rs:158
Impact: Blocks AC#6. `moves_to_go` is parsed straight into a `u64` with no clamping (engine/src/uci.rs:196, :366), so `movestogo 18446744073709551615` reaches the new periodic branch and evaluates `n + BOUNDARY_CUSHION_MOVES`. That addition overflows: debug panics with "attempt to add with overflow", release wraps the divisor to 0 and panics with "attempt to divide by zero". Either way a well-formed UCI `go` line kills the engine process mid-game. The base commit handled the same input safely, so this is patch-introduced, not pre-existing. The task text explicitly requires that TASK-7 overflow safety not regress, and the periodic branch is the one place in the new code where an untrusted protocol value is used in non-saturating arithmetic — `usable_time.saturating_add(...)` and `inc.saturating_mul(n - 1)` on the neighbouring line are both hardened, so the omission looks like an oversight rather than a decision.

Reproduction: against bf811bf, parse and evaluate the command exactly as engine/src/engine.rs:161 does:

    let line = "go wtime 10000 btime 10000 winc 100 binc 100 movestogo 18446744073709551615";
    let Ok(Command::Go(TimingMode::Timed(tc))) = Parser::parse(line) else { unreachable!() };
    tc.to_move_time(1, Player::WHITE);

  - target bf811bf, debug:   panicked at engine/src/time.rs:158:33: attempt to add with overflow
  - target bf811bf, release: panicked at engine/src/time.rs:158:17: attempt to divide by zero
  - base c55508b, release:   returns 100

Expected: A large or absurd `movestogo` yields a finite allotment rather than a panic, consistent with how the clock and increment are already treated on the adjacent lines. Saturating the divisor, or clamping the horizon to something bounded before it is used, would both do it. The existing TASK-7 test does not cover this because `allocation_preserves_values_above_u32_max` varies the clock and increment while holding `movestogo` at 20; the regression test for this finding needs to vary `movestogo` itself.

Non-blocking observations, recorded for the record and not requiring action:

- AC#4 "well-formed": I accept the implementer's reading. The share cap can still bind when the increment exceeds roughly three quarters of the usable clock, but the task itself describes MAX_CLOCK_SHARE_DIVISOR as "a backstop against pathological input", and a 1000ms clock with a 5000ms increment is pathological by any reading. The sweep test covers real controls from 90+30 down to 3+2, where 3+2 already carries an increment two thirds the size of its clock and leaves the cap slack. That is sufficient evidence for the criterion as written.
- Comment quality is good throughout: the new doc comments on BOUNDARY_CUSHION_MOVES and move_time_budget, and the inline comments in the periodic branch, state the underlying reasoning rather than citing task IDs, and none of them restate the code. No task ID, acceptance criterion, or finding ID appears in any source comment. The diff adds no `#[allow]`.
- The four updated tests were each checked against their original purpose and all retain it; the split of explicit_moves_to_go_controls_allocation_and_zero_is_safe into two tests is a genuine improvement in intent.
- Benchmarks were not run: to_move_time is called once per `go` and is not on a movegen or search hot path.

Acceptance criteria status against bf811bf:
- AC#1 proven: moves_to_go_of_zero_falls_back_to_the_game_length_heuristic asserts 498ms and equality with the None control at moves 1/20/41/80, not the 7478ms share-cap maximum.
- AC#2 proven: the_increment_reserve_does_not_bind_as_a_boundary_approaches shows the allotment growing monotonically towards the boundary and exceeding the below-reserve share throughout.
- AC#3 proven: a_period_arrives_at_its_boundary_with_time_still_on_the_clock simulates five representative controls, asserting flat allocation and non-empty arrival.
- AC#4 proven, subject to the reading above.
- AC#5 proven: movetime_holds_back_the_same_overhead_as_the_clock covers the saturation to zero, and engine/src/engine.rs:164 routes MoveTime through the helper.
- AC#6 not met: see REV-1-01.

Verification (all run by the reviewer on bf811bf, not taken from the handoff):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings, re-confirmed with a clean CARGO_TARGET_DIR since the first run was cached
- cargo test --workspace: pass, 0 failed
- Target immutability: bf811bf is an ancestor of tip 25e18ac, and the only file changed between them is the task file
---
<!-- COMMENTS:END -->
