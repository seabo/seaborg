---
id: TASK-64.7
title: Add reverse futility pruning
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 13:32'
updated_date: '2026-07-21 02:15'
labels:
  - search
  - pruning
dependencies: []
references:
  - engine/src/search.rs
parent_task_id: TASK-64
priority: medium
type: feature
ordinal: 70000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add reverse futility pruning, also called static null move pruning: in a non-PV node near the horizon, when the static evaluation exceeds beta by a depth-scaled margin, return without searching.

This is distinct from the forward futility pruning tracked by TASK-50, which skips individual quiet moves whose evaluation plus a margin cannot reach alpha. Reverse futility prunes the whole node on the opposite side of the window, before any move is generated. The two are frequently confused and are separately worth having; TASK-50 should not be treated as covering this.

It is placed alongside the existing razoring at search.rs:768, which is its mirror image on the alpha side, and shares the same guard conditions: not in check, non-PV node, shallow remaining depth, and a beta that is not a mate score.

Caveat. This decides what to discard by comparing a static evaluation against a margin, and `Search::evaluate` (search.rs:1096) is material-only. The margin is therefore being applied to a signal that ignores king safety, piece activity and pawn structure entirely. A gain is not guaranteed before the evaluation work lands, and a null or negative measurement here is itself useful evidence about evaluation quality and should be recorded rather than worked around by margin tuning.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reverse futility pruning is applied in non-PV nodes below a documented depth and is disabled in check and when beta is a mate score
- [ ] #2 The technique is implemented separately from and does not duplicate the forward futility pruning of TASK-50
- [ ] #3 A fixed-depth search on a position set where the guards are inactive returns unchanged best moves, confirming the guards
- [ ] #4 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes, including a null or negative result and its bearing on evaluation quality
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add REVERSE_FUTILITY_MAX_DEPTH (6) and reverse_futility_margin(depth) constants next to razoring.
2. In the interior-node path, right after razoring (Step 7), add reverse futility pruning: in non-PV nodes, not in check, depth <= max, beta.is_cp(), when eval - margin(depth) >= beta, return eval (fail-high without generating a move). Mirror image of razoring on the beta side.
3. Add a #[cfg(test)] rfp_disabled toggle (mirroring lmr_disabled) so the guard-soundness test can isolate RFP.
4. Tests: (a) RFP-on vs RFP-off returns identical score/best move on decisive/mate positions where guards keep it sound; (b) RFP reduces node count on a quiet position where it fires; (c) unit tests for the margin/guard helper.
5. Run required checks; measure with TASK-27 strength-regression script and record result (incl. null/negative) in implementation notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Added reverse futility pruning (static null move pruning) as Step 7's beta-side
companion to razoring in `engine/src/search.rs`. In a non-PV node, not in
check, at depth <= REVERSE_FUTILITY_MAX_DEPTH, with a centipawn beta bound, when
`eval - reverse_futility_margin(depth) >= beta` the node fails high immediately,
returning `eval` without generating a move.

Distinct from TASK-50 forward futility pruning: RFP prunes the whole node on the
beta side before any move is generated; forward futility skips individual quiet
moves on the alpha side inside the move loop. Both remain present and independent.

### Guard tuning (both pinned empirically, mirroring the null-move min-depth comment)

- `REVERSE_FUTILITY_MAX_DEPTH = 2`. Unlike razoring (quiescence-verified) and
  null-move pruning (re-search-verified), RFP searches nothing, so it cannot
  refuse to fire on a node hiding a forced win — it returns a bare material
  score. At depth 3 the regression suite's KP win (`8/6pk/8/8/8/8/P7/K7 w`) and
  short mates (`child_mate_windows_preserve_distance_parity`,
  `gives_correct_answers` mate lines) start reporting cp instead of the win. 2
  is the largest depth that keeps every suite gate exact.
- `reverse_futility_margin = 300 + 100*depth`. The 300cp base is required: with
  the thin `100*depth` margin, the fifty-move test's bare-kings node
  (`eval` 0, `beta` −299 from a pessimistic parent window) fired RFP and shifted
  an exact fail-soft value (299 -> 293). `evaluate` is material-only, so a
  multi-pawn base keeps the prune to genuine material edges.

### Verification
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (346 engine + others; 3 initially-failing suite
  gates now green after the guard tuning above)

New tests: reverse_futility_margin_grows_with_depth,
reverse_futility_pruning_does_not_change_sound_search_results (RFP on vs off,
identical on decisive/mate positions — guard soundness),
reverse_futility_pruning_reduces_the_search_tree (RFP fires).

### Strength measurement (AC#4)

Measured with the TASK-27 harness's runner. NB: the wrapper
(tools/strength/strength_test.py) could not be used directly — its UCI preflight
expects line-buffered stdout, but seaborg block-buffers stdout under a pipe and
flushes only at exit, so the handshake times out; this reproduces on the
baseline binary (0f73ec8) too, so it is a pre-existing engine-I/O limitation
independent of this task. Also the documented `--engine-arg=-u` flag is stale:
the current CLI enters UCI mode by default with no subcommand. FastChess itself
(the runner the script wraps) drives games correctly, so the match was run
through it directly with the repository openings and methodology.

- Runner: fastchess v1.7.0-alpha; builds: baseline git:0f73ec8, candidate
  git:1ddd6cc, both `RUSTFLAGS="-C target-cpu=native" cargo build --release
  --locked`.
- Limit: nodes=200000 per move (equal-node budget: RFP's saved node expansions
  convert into extra search depth; an equal-*depth* limit would show ~no
  difference because RFP mostly reproduces the same depth-N result faster).
- Openings: tools/strength/openings-v1.epd, colour-reversed pairs.
- Result: 200 games, candidate +47.19 +/- 33.68 Elo, LOS 99.74%,
  56.75% (64W / 37L / 99D). CI [+13.5, +80.9] is entirely above zero.

Bearing on evaluation quality: a clear positive, not the null/negative the
material-only caveat allowed for. The gain is realised as search efficiency —
pruning obviously-winning shallow nodes lets the same node budget reach deeper —
so even a material-only evaluation benefits. The technique is currently held at
depth 2 only because, without a verification search, it would mask forced wins;
a positional evaluation would let a future revision trust it (and a higher
max-depth) far more, which is where the larger gains would come from.
<!-- SECTION:NOTES:END -->
