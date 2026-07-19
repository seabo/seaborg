---
id: TASK-64.14
title: Replace material-only evaluation with a tapered hand-crafted evaluation
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 13:33'
updated_date: '2026-07-19 20:18'
labels:
  - evaluation
  - strength
  - nnue
dependencies: []
references:
  - engine/src/eval.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: feature
ordinal: 77000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The static evaluation is material only. Replace it with a tapered hand-crafted evaluation carrying at minimum piece-square tables and a game-phase interpolation.

Current state. `material_evaluation` (eval.rs:32-43) is a popcount of five piece types against fixed values, with knight and bishop both at 300 (eval.rs:5-6). There is no piece-square term, no mobility, no king safety, no pawn structure, no bishop pair, no tempo, and no game-phase tapering. `Search::evaluate` (search.rs:1095-1097) wraps it and applies the side-to-move sign.

Why this sits inside the search programme rather than after it. Several techniques here decide what to prune by comparing this evaluation against a margin: razoring, reverse futility, futility, and the delta cut in quiescence. A material-only evaluation makes those comparisons close to arbitrary, because it cannot distinguish a position where the side to move is materially level and positionally lost from one where they are level and winning. The margin-based tasks in this programme are expected to under-deliver until this lands, and their measurements should be revisited afterwards.

Why it matters specifically for NNUE. Training labels distilled from self-play inherit the evaluation at the leaves, refined by search. Distilling a deep, highly selective search over a material-only leaf evaluation produces labels that are sharper about tactics and nearly silent about positional judgement, which is most of what the network is wanted for. Piece-square tables are the minimum that gives the search something positional to propagate.

Piece-square tables are also the natural first incremental term, which is why the incremental evaluation seam is scheduled immediately after this rather than before it: material and piece-square scores update trivially on make and unmake, and getting that shape right here makes the NNUE accumulator a substitution rather than a new mechanism.

One constraint carries over unchanged. The evaluation must remain position-intrinsic: it must not read the halfmove clock or any other state the Zobrist key does not cover. The reasoning is documented at search.rs:1077-1093 and the invariant is load-bearing for transposition-table reuse; TASK-58 removed a clock-dependent term for exactly this reason and it must not return.

Scope beyond piece-square tables and tapering, such as mobility, king safety and pawn structure, is a decision to make and record. Tuning method is likewise open.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The evaluation includes piece-square tables interpolated between a middlegame and an endgame phase
- [ ] #2 Knight and bishop no longer carry identical values, and the values used are recorded
- [ ] #3 The evaluation remains position-intrinsic and reads no state outside the Zobrist key, with a test asserting invariance to the halfmove clock
- [ ] #4 The set of evaluation terms implemented beyond piece-square tables is recorded with rationale
- [ ] #5 The tuning method used to fix the parameters is recorded
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
- [ ] #7 Margin-based pruning tasks already landed are re-measured against the new evaluation and any margin revisions are recorded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Decisions (recorded per AC#4/#5): (a) scope = material + PST only, tapered MG/EG; (b) parameters = adopt published Texel-tuned PeSTO (rofChade) MG/EG piece values + PSTs, cited; (c) strength = fixed-depth smoke SPRT via tools/strength (non-authoritative, directional).

1. Rewrite engine/src/eval.rs: add PeSTO MG/EG piece-value arrays (knight != bishop) + MG/EG piece-square tables (published a8=0 orientation) + per-piece game-phase increments. Implement tapered_eval: sum White-Black of (material+PST) for MG and EG, interpolate by game phase (0..24), return White-relative i16. Keep PIECE_VALUES/piece_value unchanged for SEE (deliberately separate exchange values; documented). Rename trait method to a static-eval name.
2. Update Search::evaluate (search.rs:1268) to call the new method; keep the position-intrinsic contract (no halfmove-clock read).
3. Tests: rewrite the halfmove-clock invariance test to assert clock-invariance without pinning the material-only 900 (AC#3); add a colour-mirror symmetry test (mirrored position evaluates to the negation; startpos==0) to catch PST orientation errors; add a phase-interpolation test (a MG-heavy vs EG position taper correctly). Fix search tests that hardcode material-only scores (e.g. quiescence check-evasion expected value).
4. Verify: cargo fmt --check, clippy -D warnings, cargo test --workspace.
5. Strength: build baseline (master) + candidate release binaries, run tools/strength/strength_test.py in fixed-depth smoke mode, record report path + W/D/L/Elo in implementation notes (AC#6). AC#7: only razoring margin is landed; re-measure/record whether the razoring constant needs revision under the new eval, else record no revision.
6. Record AC#2 values, AC#4 term set + rationale, AC#5 tuning method in implementation notes.
<!-- SECTION:PLAN:END -->
