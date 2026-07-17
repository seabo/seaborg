---
id: TASK-9
title: Correct quiescence search check and TT semantics
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 20:57'
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
1. Correct the in-check quiescence abort path so stopping cannot be misclassified as checkmate.
2. Add a regression test covering a pre-signalled stop with legal check evasions.
3. Run formatting, focused quiescence tests, and the full workspace suite; commit the rework and record a new immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented complete legal evasion search at checked quiescence nodes, with mate detection and repetition/fifty-move termination. TT hits now apply only as non-PV alpha-beta bounds; stored search scores are never reused as static evaluation. Added focused regression coverage for quiet evasions, horizon mate, exact/lower/upper TT hits, null TT moves, and PV static-evaluation separation.

Resolved REV-1-01: quiescence now distinguishes signature-valid TT hits from clashes, retains clash slots only for replacement, and ignores their stored bounds. Added a forced one-slot collision regression test. Verification passed: cargo fmt --check; cargo test -p engine quiescence -- --nocapture (5 passed); cargo test --workspace (all passed, 1 ignored).

Resolved review comment #1: quiescence now decides checkmate from the generated legal-evasion list before consulting the stop signal, so an abort with available evasions returns the current alpha bound. Added a deterministic abort regression. Verification passed: cargo fmt --check; cargo test -p engine quiescence -- --nocapture (6 passed); cargo test --workspace (all passed, 1 ignored).
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

author: @codex
created: 2026-07-17 20:21
---
Implementation handoff
Branch: task-9-quiescence-semantics
Worktree: /Users/seabo/seaborg-worktrees/task-9-quiescence-semantics
Base: 4e7c7089431de8122541bc430ff200beb954f2e1
Implementation target: d4d2ae6
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: passed
- cargo test -p engine quiescence -- --nocapture: passed (5 tests)
- cargo test --workspace: passed (1 ignored)
Known failures: none
---

author: @codex
created: 2026-07-17 20:55
---
Rework claim: resolving review comment #1, which reports a false mate score when quiescence stops before searching the first legal check evasion.
---

author: @codex
created: 2026-07-17 20:57
---
Implementation handoff
Branch: task-9-quiescence-semantics
Worktree: /Users/seabo/seaborg-worktrees/task-9-quiescence-semantics
Base: 4e7c7089431de8122541bc430ff200beb954f2e1
Implementation target: 5cea705a7e781078d4c30f21b182cd71daa26636
Resolved findings: review comment #1 (no REV ID assigned)
Verification:
- cargo fmt --check: passed
- cargo test -p engine quiescence -- --nocapture: passed (6 tests)
- cargo test --workspace: passed (1 ignored)
Known failures: none
---
<!-- COMMENTS:END -->
