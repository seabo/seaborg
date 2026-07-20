---
id: TASK-50
title: Add null move and futility pruning to the main search
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-20 20:15'
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
- [x] #1 Futility pruning is implemented at step 8 and is disabled in PV nodes and when in check
- [x] #2 Null move pruning with a verification search is implemented at step 9 and is disabled in PV nodes, when in check, and in likely-zugzwang positions
- [x] #3 Both techniques are measured with the TASK-27 strength-regression script and show no strength loss, with results recorded in the implementation notes
- [x] #4 A fixed-depth search on a known position set returns the same best moves as before where pruning is inactive, confirming the guards
- [x] #5 The evaluation-quality assessment is recorded, including the decision to proceed or to defer
- [x] #6 The step 8 and step 9 TODO markers are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reuse this Changes-Requested branch/worktree; integrate current master (pinned 02e3ba5, which carries TASK-20's core->chess rename) into the branch via merge.
2. Resolve merge conflicts, letting git relocate core/src/position/mod.rs changes onto chess/src/position/mod.rs.
3. Fix the two NEW test functions in engine/src/search.rs to call chess::init::init_globals() instead of core::init::init_globals(); sweep for any other core:: paths the diff introduced.
4. Re-run required checks: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace.
5. Commit the integrated result; record rework notes and a fresh handoff (base pinned 02e3ba5, new implementation target SHA). Prior approval of e6acc56 is invalidated.
6. Move to In Review for a fresh independent review.
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

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-20 19:37
---
## Review verdict: APPROVED

Reviewer: independent review per TASK_LIFECYCLE.md
Base: ba6aec1
Implementation target (code SHA): e6acc56
Branch: task-50-null-move-futility-pruning

### Verification (run on e6acc56 in the task worktree)
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (fresh CARGO_TARGET_DIR, no warnings)
- cargo test --workspace: PASS (engine 294 passed / 2 pre-existing ignored; all crates green)

### Acceptance criteria
- AC#1 futility (non-PV, not in check): implemented at Step 8, guards verified in code and by forward_pruning_does_not_change_sound_search_results.
- AC#2 null move + verification (non-PV, not in check, zugzwang via has_non_pawn_material): implemented at Step 9; reduced-depth verification search suppresses NMP at the verifying ply via nmp_min_ply.
- AC#3 strength: TASK-27 node-limited runs candidate vs base ba6aec1 recorded in notes; no strength loss, 0 crashes/forfeits (authoritative timed-self-play gate not runnable per docs, honestly caveated).
- AC#4 fixed-depth equivalence: forward_pruning_does_not_change_sound_search_results asserts identical score+best move with pruning on vs off across 5 positions.
- AC#5 eval-quality assessment recorded (tapered material+PST -> PROCEED).
- AC#6 both TODO markers replaced; numbered Step 8/9/10 comments retained.

### Notes checked
- Null-window bounds mirror the Step 19 convention (width-1 window around beta); return path clamps unproven mate to the beta bound.
- Temporary Move::null() marker in stack[ply].mov is overwritten per move in the loop, so it cannot leak into loop children.
- Depth is i16, so reduction subtraction cannot underflow-panic; verification search terminates (reduced depth, no self-recursion at the verifying ply).
- Movegen/make-unmake/perft hot paths are byte-identical (position changes are purely additive); perft/movegen speed benches are not applicable to this diff.
- No code comment cites a task ID, AC, or finding ID.

Verdict: all acceptance criteria proven, no blocking findings. Approving e6acc56 as the immutable code target. Moving to Ready to Merge.
---

author: @codex
created: 2026-07-20 20:12
---
## Merge ejected: integrated result fails to compile

Merge attempted by $merge and ejected. Primary (master) was NOT advanced; it remains at the tip tested below.

### Environment
- Primary tip tested (T): 02e3ba5
- Approved implementation target: e6acc56 (base ba6aec1)
- Trial merge M: non-ff merge of task tip 2d9ba26 into detached T (discarded)

### Failing command and evidence
`cargo test --workspace --no-run` on the trial merge M:
```
error[E0433]: cannot find `init` in `core`   (x2)
error: could not compile `engine` (lib test) due to 2 previous errors
```

### Root cause
TASK-20 (crate rename core -> chess, commit e06cae8) landed on master *after* this task branched from ba6aec1. master's engine crate now depends on `chess` (engine/Cargo.toml: `chess = { path = "../chess" }`), and its search.rs tests call `chess::init::init_globals()`. This task's two NEW tests still call `core::init::init_globals()`:
- engine/src/search.rs: fn forward_pruning_does_not_change_sound_search_results
- engine/src/search.rs: fn forward_pruning_reduces_the_search_tree

git's rename detection correctly relocated the core/src/position/mod.rs changes to chess/src/position/mod.rs and converted master's own `core::` references, but it left these two task-added lines untouched (master never had them), so they reference a crate path that no longer resolves. Textually clean, semantically broken.

### Required rework ($implement)
Re-target this task onto the current master tip (pin the base SHA per the merge-time base-drift guidance) and update the two new tests to `chess::init::init_globals()` (and any other core:: paths the diff introduces). Re-run fmt/clippy/test, then hand back for a fresh review — the prior approval of e6acc56 is invalidated because the integrated code target must change.

The merge skill does not edit implementation code, so no fix is applied here.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented step 8 futility pruning and step 9 null-move pruning with verification in engine/src/search.rs, backed by a new make_null_move/unmake_null_move primitive and a has_non_pawn_material zugzwang proxy on core Position, plus Score::dec_one for the null window. All guards match the ACs: futility is non-PV / not-in-check / cp-alpha / near-horizon; null move adds eval>=beta, non-pawn material, and no-consecutive-null, with a reduced-depth verification search (NMP suppressed at the verifying ply) above NULL_MOVE_VERIFY_DEPTH. NULL_MOVE_MIN_DEPTH=5 preserves exact shallow-mate detection (documented at the constant). Verified on target e6acc56: cargo fmt --check PASS; cargo clippy --workspace --all-targets --all-features -- -D warnings PASS with a clean CARGO_TARGET_DIR; cargo test --workspace PASS (engine 294 passed, 2 pre-existing ignored). Guard-equivalence and tree-reduction tests confirm the pruning fires yet leaves sound-position results unchanged; TASK-27 node-limited strength runs (candidate vs base ba6aec1) show no loss.
<!-- SECTION:FINAL_SUMMARY:END -->
