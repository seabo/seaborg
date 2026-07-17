---
id: TASK-9
title: Correct quiescence search check and TT semantics
status: Changes Requested
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 19:27'
labels:
  - search
  - correctness
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/tt.rs
priority: high
type: bug
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Quiescence currently allows stand-pat behavior while in check and reuses transposition-table search scores as static evaluations without sufficient bound or depth semantics. Restore legal check-evasion behavior and valid alpha-beta windows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Positions in check never return a stand-pat cutoff and search all required legal evasions
- [ ] #2 Transposition-table values are used in quiescence only when their stored depth and bound semantics justify the use
- [ ] #3 A stored search score is not substituted for a static evaluation unless it was explicitly stored as one
- [ ] #4 Quiescence never recurses with an empty or inverted alpha-beta window
- [ ] #5 Regression tests cover quiet check evasions, checkmate at the horizon, and TT hit variants
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Refactor quiescence TT probing so every hit meets the encoded quiescence depth requirement (depth zero) and scores are applied only as non-PV alpha-beta bounds, never as stand-pat evaluations.
2. Separate in-check quiescence from stand-pat logic and search every legal evasion, returning mate when none exist and terminating draw cycles.
3. Add focused regression tests for quiet evasions, horizon mate, TT exact/lower/upper hits, null stored moves, and static-evaluation separation.
4. Run formatting and workspace tests, commit the implementation, then record an immutable In Review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented complete legal evasion search at checked quiescence nodes, with mate detection and repetition/fifty-move termination. TT hits now apply only as non-PV alpha-beta bounds; stored search scores are never reused as static evaluation. Added focused regression coverage for quiet evasions, horizon mate, exact/lower/upper TT hits, null TT moves, and PV static-evaluation separation.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 19:14
---
Implementation handoff
Branch: task-9-quiescence-semantics
Worktree: /Users/seabo/seaborg-worktrees/task-9-quiescence-semantics
Base: 4e7c7089431de8122541bc430ff200beb954f2e1
Implementation target: b132b08e55530074d8cef14a19101024d7814dfa
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo test -p engine quiescence: passed (4 tests)
- cargo test -p engine gives_correct_answers: passed
- cargo test --workspace: passed (including long-running perft; 1 ignored)
Known failures: none
---

author: @codex
created: 2026-07-17 19:27
---
Review attempt: 1
Reviewed branch: task-9-quiescence-semantics
Reviewed implementation: b132b08e55530074d8cef14a19101024d7814dfa
Verdict: changes_requested

REV-1-01 [P1] Quiescence consumes TT slot clashes as valid hits
Location: engine/src/search.rs:798
Impact: A position whose table index is occupied by a different signature reads that other position's exact/lower/upper score at lines 808-832. This can return an unrelated exact score, cause an invalid cutoff, or narrow the alpha-beta window from data that is not a TT hit, violating acceptance criterion 2 and leaving criterion 5 without clash coverage.
Reproduction: Table::probe returns Probe::Clash when the indexed live entry has a different signature (engine/src/tt.rs:415-422). The Clash arm in quiesce returns the same WritableEntry as Hit, and WritableEntry::read validates only generation, not signature, so !entry.is_empty() is true and the bound is applied.
Expected: Preserve the writable slot for replacement but gate score/bound use on Probe::Hit only; add a regression test proving a clash cannot affect a quiescence result.

Verification:
- git diff --check 4e7c7089431de8122541bc430ff200beb954f2e1..b132b08e55530074d8cef14a19101024d7814dfa: passed
- cargo fmt --check: passed
- cargo test -p engine quiescence -- --nocapture: passed (4 tests)
- cargo test --workspace: passed (1 ignored)
- Static control-flow reproduction against Table::probe and WritableEntry::read: confirmed clash entry is non-empty and its bound is consumed
---
<!-- COMMENTS:END -->
