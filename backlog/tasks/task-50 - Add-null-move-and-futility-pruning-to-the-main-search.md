---
id: TASK-50
title: Add null move and futility pruning to the main search
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-20 19:28'
labels: []
dependencies:
  - TASK-46
references:
  - engine/src/search.rs
priority: medium
type: feature
ordinal: 50000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search steps 8 and 9 are unimplemented placeholders. Both are forward-pruning techniques that share the same guard conditions (not in check, non-PV node, a usable static evaluation) and the same validation burden, so they are delivered and measured together.

Step 8, futility pruning: near the horizon, skip quiet moves whose static evaluation plus a margin cannot reach alpha.
Step 9, null move search with verification: give the opponent a free move at reduced depth; if the result still fails high, prune. Verification search is required to avoid zugzwang blunders.

The numbered Step N comments in search.rs are a deliberate map of the intended search structure. Replace the TODO markers with implementations; do not delete the step comments.

Sequencing: this is gated on TASK-46 because pruning decisions that read alpha and the transposition table compound the aborted-subtree score problem rather than tolerate it. This ticket and TASK-51 and TASK-52 must land one at a time, since concurrent search changes make strength-regression attribution impossible.

Caveat to check first: these prunings assume the static evaluation is informative enough for pruning decisions to be trustworthy. Assess the current evaluation before committing to the approach, and report back if it is too coarse (for example material-only) for a gain to be expected.

TODO sites: engine/src/search.rs:595 (futility), engine/src/search.rs:598 (null move).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Futility pruning is implemented at step 8 and is disabled in PV nodes and when in check
- [ ] #2 Null move pruning with a verification search is implemented at step 9 and is disabled in PV nodes, when in check, and in likely-zugzwang positions
- [ ] #3 Both techniques are measured with the TASK-27 strength-regression script and show no strength loss, with results recorded in the implementation notes
- [ ] #4 A fixed-depth search on a known position set returns the same best moves as before where pruning is inactive, confirming the guards
- [ ] #5 The evaluation-quality assessment is recorded, including the decision to proceed or to defer
- [ ] #6 The step 8 and step 9 TODO markers are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Evaluation assessment: confirm eval is tapered material+PST (MG/EG by phase), not material-only, so futility/null-move margins are meaningful. Record decision to proceed.
2. Core: add Position::make_null_move/unmake_null_move (flip side to move, clear ep, bump halfmove clock + move number, recompute State, push/pop a NULL UndoableMove). No piece placement changes. Debug-assert not-in-check precondition and make/unmake symmetry. Unit-test zobrist round-trip, state restore, and repetition-scan parity.
3. Search: add make_null_move/unmake_null_move wrappers carrying the White-relative eval accumulator across unchanged.
4. Step 8 futility pruning: shared guards (non-PV, not in check, usable cp eval). Near horizon, skip quiet moves whose eval + depth-scaled margin cannot reach alpha; keep best_value as the futility bound. Never near mate scores. Decision computed at the Step 8 site, applied in the move loop.
5. Step 9 null-move pruning with verification: guards (non-PV, not in check, eval >= beta, side has non-pawn material to avoid zugzwang, not already a null-move reply). Reduced-depth null search; on fail-high, run a verification search at reduced depth and prune only if it also fails high. Retain numbered step comments; replace only the TODO markers.
6. Tests: node/depth-fixed best-move equivalence where guards disable pruning; unit tests for margins and zugzwang guard.
7. Verification: cargo fmt --check, clippy -D warnings, cargo test --workspace. Run node-limited fastchess match (base ba6aec1 vs candidate) for a strength signal; record W/D/L and Elo estimate in notes with the timed-self-play caveat.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Evaluation-quality assessment (AC #5)

Assessed engine/src/eval.rs before committing to the approach. The static evaluation is a tapered material + piece-square-table score: MG and EG values per piece type interpolated by a game-phase counter (EvalState::score). It is materially richer than the material-only case the task caveat warns about, and it is position-intrinsic (clock-independent) as the pruning steps require. Decision: PROCEED. Both futility and null-move pruning have an informative enough signal for a gain to be plausible, which the strength run below bears out.

## Strength measurement (AC #3)

Measured with the TASK-27 script (tools/strength/strength_test.py, FastChess) comparing candidate e6acc56 vs base ba6aec1, both built RUSTFLAGS='-C target-cpu=native' cargo build --release. Node-limited games (deterministic, and the correct budget for a pruning change: fixed nodes -> deeper search is where the benefit appears). Candidate is player 1.

- nodes=200000, 20 games: W12 D4 L8->L4 (W12/D4/L4), Elo +147.2 ±99.9, pentanomial [0,0,4,4,2], 0 forfeits, 0 crashes.
- nodes=500000, 20 games: W10/D2/L8, Elo +34.9 ±110.2, pentanomial [0,2,6,0,2], 0 forfeits, 0 crashes.
- Combined 40 games: W22/D6/L12 (net +10). Both point estimates positive; neither shows a regression; zero crashes/forfeits/illegal moves (confirms the added internal null moves never surface as an illegal bestmove or hang).

Result: no strength loss (weakly positive). Caveat, surfaced honestly: the harness cannot run its AUTHORITATIVE gate for seaborg. Authoritative mode requires timed self-play, which docs/strength-testing.md documents seaborg cannot yet do reliably; smoke mode is capped at 20 games and openings-v1.epd holds only 8 positions, so the sample is small and the Elo error is wide. The measurement establishes no-loss as far as the harness permits, not an authoritative SPRT PASS.

## Design note: null-move min depth

Standard low null-move min-depth masks the repository's exact shallow-mate regression tests (search::tests::gives_correct_answers and child_mate_windows_preserve_distance_parity): a null cutoff is a fail-high bound, so where it fires near the horizon it replaces a forced mate the node was about to prove, and the reduced null search is too shallow to see that mate. NULL_MOVE_MIN_DEPTH is set to 5 — the minimal value that keeps the whole mate contract green (4 fails, 5 passes) — which leaves the last plies before the horizon searched in full while still pruning the large, expensive subtrees higher up. Rationale documented at the constant. Reviewers: this is the one non-obvious tuning choice.

Correction: the nodes=200000 batch result is exactly W12 / D4 / L4 (the 'L8->L4' fragment above was a typo).

## Implementation handoff
Branch: task-50-null-move-futility-pruning
Worktree: /Users/seabo/seaborg-worktrees/task-50-null-move-futility-pruning
Base: ba6aec1
Implementation target: e6acc56
Resolved findings: none (new work)
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (no warnings)
- cargo test --workspace: PASS (all suites; engine 294 passed, 2 pre-existing ignored)
- TASK-27 strength script (node-limited smoke, e6acc56 vs ba6aec1): no strength loss, +147.2/+34.9 Elo across two 20-game batches, 0 crashes/forfeits (see Strength measurement note; authoritative gate not runnable for seaborg per docs)
Known failures: none

Scope note for review: implementing null-move pruning required a null-move primitive on core Position (make_null_move/unmake_null_move). This was anticipated by the codebase — replay_last_move_deltas already documented the null-move carry-across as 'a constraint on whichever change introduces them' — so it is treated as in-scope additive work, not a structural change. The one non-obvious tuning decision (NULL_MOVE_MIN_DEPTH=5 to preserve exact shallow-mate detection) is documented at the constant and in the design note above.
<!-- SECTION:NOTES:END -->
