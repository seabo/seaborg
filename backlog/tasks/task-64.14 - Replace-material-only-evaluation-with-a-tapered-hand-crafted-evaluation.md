---
id: TASK-64.14
title: Replace material-only evaluation with a tapered hand-crafted evaluation
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 13:33'
updated_date: '2026-07-19 21:10'
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
- [x] #1 The evaluation includes piece-square tables interpolated between a middlegame and an endgame phase
- [x] #2 Knight and bishop no longer carry identical values, and the values used are recorded
- [x] #3 The evaluation remains position-intrinsic and reads no state outside the Zobrist key, with a test asserting invariance to the halfmove clock
- [x] #4 The set of evaluation terms implemented beyond piece-square tables is recorded with rationale
- [x] #5 The tuning method used to fix the parameters is recorded
- [x] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
- [x] #7 Margin-based pruning tasks already landed are re-measured against the new evaluation and any margin revisions are recorded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Integration rework plan:
1. Commit the Changes Requested claim, then merge the current primary tip c7826f1 into the persistent task branch so the newly landed evaluation benchmark is part of the reviewable target.
2. Update benches/search.rs to call static_eval() and revise its description to match the tapered evaluator; run the focused benchmark to verify it still measures static evaluation.
3. Run cargo fmt --check, strict workspace Clippy, and cargo test --workspace; commit the integrated implementation and record resolution evidence for the merge-gate failure.
4. Record a new immutable implementation target and task-only In Review handoff for independent review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a tapered piece-square evaluation replacing the material-only popcount.

Design decisions (surfaced to and chosen by the user):
- Scope (AC#4): material + piece-square tables only, tapered between a middlegame and an endgame phase. No mobility/king-safety/pawn-structure terms. Rationale: the task frames PSTs as the minimum; keeping the term set minimal keeps the surface small, keeps every term trivially incrementally updatable (the seam scheduled next), and makes the eventual NNUE accumulator a substitution rather than a new mechanism. Terms implemented: per-piece middlegame/endgame material values + per-piece middlegame/endgame piece-square tables, blended by a game-phase weight.
- Tuning method (AC#5): adopted the published Texel-tuned PeSTO ("Piece-Square Tables Only") parameter set by Ronald Friederich (rofChade), reproduced from the Chess Programming Wiki and verified digit-for-digit against that source. These were fitted by logistic regression against game outcomes; no hand-tuning was applied. They are used as a coherent set (MG/EG material values, MG/EG PSTs, and game-phase increments).
- Distinct knight/bishop values (AC#2): knight MG 337 / EG 281; bishop MG 365 / EG 297 (centipawns). Bishop > knight in both phases. Full recorded value set: pawn 82/94, knight 337/281, bishop 365/297, rook 477/512, queen 1025/936, king 0/0. Game-phase increments: pawn 0, knight 1, bishop 1, rook 2, queen 4, king 0 (summing to 24 at the opening). The static-exchange-evaluation values in eval.rs (PIECE_VALUES, knight=bishop=300) are deliberately left unchanged and documented as separate exchange prices; this isolates the eval change so the strength measurement below attributes cleanly.

Implementation:
- engine/src/eval.rs: PeSTO MG/EG value arrays, MG/EG PST arrays (stored in published a8=0 orientation), game-phase increments, and tapered_evaluation summing White-minus-Black (material+PST) for both phases and interpolating by phase (saturated at 24). Trait method renamed material_eval -> static_eval; returns a White-relative centipawn score.
- engine/src/search.rs: Search::evaluate now calls static_eval; the position-intrinsic contract (no halfmove-clock read) is preserved and its documentation retained.

Position-intrinsic invariant (AC#3): evaluation reads only piece placement and colour (all Zobrist-covered). Test static_evaluation_is_independent_of_the_halfmove_clock asserts the score is identical at clocks 0/50/99. Added the_evaluation_is_symmetric_under_a_colour_mirror (a position and its colour-and-rank mirror score exactly opposite; catches PST orientation bugs) and piece_square_scores_are_tapered_by_game_phase (a central king is penalised with heavy pieces on the board but rewarded once they are gone; only tapering can express both).

Test updates: search tests that pinned material-only scores were updated to the tapered values, with comments rewritten to explain the number rather than cite the old material figure. Bare-king positions used as fixed-value anchors were switched from 7k/8/8/8/8/8/8/K7 (asymmetric under PSTs, evaluates to -10) to the colour-symmetric k7/8/8/8/8/8/8/K7 (evaluates to exactly 0), keeping the existing 'true value is 0' reasoning literally correct.

Strength measurement (AC#6): per the user's choice, a fixed-depth smoke SPRT via tools/strength/strength_test.py (FastChess alpha 1.5.0), candidate git:88b78c0 vs baseline git:aa915d8 (the branch point), depth=4, 20 games. Result: candidate 19 wins, 0 draws, 1 loss; pentanomial [0,0,1,0,9]; Elo point estimate +511.5; LLR 0.2 within [-2.94, 2.94]. Verdict INCONCLUSIVE, which is smoke mode's mandatory non-authoritative label and the game cap (20), not a weak signal: 19-1 at equal fixed depth is a decisive directional confirmation that the tapered eval is much stronger than material-only. Report archived at /tmp/seaborg-strength-64_14/artifacts-smoke/report.json. An authoritative timed SPRT is out of scope here: it requires a time-based limit and commonly thousands of games, and the strength doc cautions seaborg's fast-time-control play is not yet reliable; fixed depth deliberately removes that confound.

Margin re-measurement (AC#7): of the margin-based pruning techniques named in the task (razoring, reverse futility, futility, quiescence delta), only razoring is currently landed (should_razor, search.rs; margin 426 + 252*depth*depth). Futility, reverse futility, null-move and quiescence delta pruning are still unimplemented TODO stubs, so there is nothing else to re-measure. The razoring comparison is eval + margin < alpha, on the same centipawn scale as before; the new evaluation does not change that scale, and the smoke games ran with razoring active and no misbehaviour. No razoring-margin revision is made here: choosing an optimal margin under the new evaluation is a tuning question that needs an authoritative timed SPRT and belongs to the razoring-margin task (TASK-64.4). Recorded: no margin revision.

Verification (on target 88b78c0):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings)
- cargo test --workspace: pass (266 engine + 43 core + others; 0 failed)

Razoring re-measurement (AC#7, superseding the earlier scale-only rationale): compared the retained margin (426 + 252*depth^2 through depth 6) directly against an otherwise identical tapered-evaluation build with should_razor forced false. TASK-27 harness, FastChess alpha 1.5.0, fixed depth 6, paired colour reversal, 20 games: retained-margin candidate 10 wins, 0 draws, 10 losses; pentanomial [0,0,10,0,0]; Elo estimate 0.0; LLR 0.0 within [-2.94,2.94]; zero crashes/forfeits. Every opening pair split 1-1, so this capped smoke sample detects no strength change attributable to razoring. Decision: retain the landed margin unchanged; the evidence does not justify a revision. This is a non-authoritative smoke measurement, not a parameter-tuning SPRT. Artifact: /tmp/seaborg-strength-64_14-rework/artifacts-razoring-depth6/report.json. Comparator SHA-256 04bdb6c538b1d1f0f5b87ab737898ba569a080c6a8d5f2fa5c2361b4e756b17e; retained-margin SHA-256 102d861cdee6261080a41b2554c73faf8c31cee5e591fae727faff5eba279fc7.

Resolved REV-1-01: added the attributable current-margin-versus-disabled comparison requested by review, recorded the neutral result and explicit no-revision decision. The comparator was temporary; engine/src/search.rs is restored exactly to implementation target 88b78c0, so the immutable code target is unchanged.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 20:43
---
Implementation handoff
Branch: task-64.14-tapered-eval
Worktree: /Users/seabo/seaborg-worktrees/task-64.14-tapered-eval
Base: aa915d85d32d03d829d0636c6af3e71b40a6632f
Implementation target: 88b78c0
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings)
- cargo test --workspace: pass (0 failed)
- fixed-depth smoke SPRT (depth=4, 20 games), candidate 88b78c0 vs baseline aa915d8: 19W-0D-1L, Elo est +511.5, verdict INCONCLUSIVE (smoke is non-authoritative and capped at 20 games); report at /tmp/seaborg-strength-64_14/artifacts-smoke/report.json
Known failures: none. Note: base is the branch point aa915d8; master has since advanced to df6f373, so the merge gate should re-integrate onto the live tip.
---

author: @codex
created: 2026-07-19 20:47
---
Review attempt: 1
Reviewed branch: task-64.14-tapered-eval
Reviewed implementation: 88b78c0
Verdict: changes_requested

REV-1-01 [P1] The landed razoring margin was not re-measured
Location: TASK-64.14 AC #7 and implementation notes
Impact: AC #7 explicitly requires already-landed margin-based pruning to be re-measured against the new evaluation and any margin revision to be recorded. The only strength run compares the complete tapered evaluator at 88b78c0 with the material-only base aa915d8 while razoring is enabled in both. That result measures the evaluator as a whole; it does not isolate the razoring margin or show whether the margin remains appropriate under the new score distribution. The statement that the scale remains centipawns and the run showed no misbehaviour is reasoning, not a margin measurement.
Reproduction: Inspect /tmp/seaborg-strength-64_14/artifacts-smoke/report.json and the recorded command. It has only candidate 88b78c0 versus baseline aa915d8 and contains no razoring-disabled engine, alternate margin, trigger-rate telemetry, or paired comparison capable of attributing an outcome to the margin.
Expected: Re-measure the landed razoring configuration under the new evaluator with an attributable comparison (for example current margin versus razoring disabled or a justified alternate margin using the TASK-27 harness), record the result and whether the margin changes, then provide the resulting artifact/evidence. If the repository deliberately intends TASK-64.4 to own this measurement instead, that is a scope/acceptance-criterion change requiring human direction rather than checking AC #7 from the current smoke run.

Verification:
- git merge-base --is-ancestor aa915d85d32d03d829d0636c6af3e71b40a6632f 88b78c0: pass
- git diff --stat 88b78c0..d9278bd: task metadata only
- cargo fmt --check: pass
- clean-CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (43 core; 266 engine passed, 2 ignored; 19 integration; 1 doc test)
- archived fixed-depth smoke report: present and matches 19W-0D-1L, but does not isolate razoring
- hot-path benchmark: not used for the verdict because another task's cargo test/engine process was active, so the machine did not meet the required idle condition
---

author: @codex
created: 2026-07-19 20:58
---
Resolved REV-1-01: TASK-27 fixed-depth paired comparison now isolates the landed razoring margin from the tapered evaluator. Current margin vs razoring disabled finished 10W-0D-10L (0 Elo, LLR 0.0, no crashes/forfeits); margin retained unchanged. Evidence: /tmp/seaborg-strength-64_14-rework/artifacts-razoring-depth6/report.json.
---

author: @codex
created: 2026-07-19 20:58
---
Implementation handoff
Branch: task-64.14-tapered-eval
Worktree: /Users/seabo/seaborg-worktrees/task-64.14-tapered-eval
Base: aa915d85d32d03d829d0636c6af3e71b40a6632f
Implementation target: 88b78c0
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings)
- cargo test --workspace: pass (43 core; 266 engine passed, 2 ignored; 19 integration; 1 doc test; 0 failed)
- TASK-27 fixed-depth smoke comparison, depth=6, 20 games, retained razoring margin vs disabled: 10W-0D-10L, Elo 0.0, LLR 0.0, zero crashes/forfeits; report at /tmp/seaborg-strength-64_14-rework/artifacts-razoring-depth6/report.json
Known failures: none
---

author: @codex
created: 2026-07-19 21:03
---
Review attempt: 2
Reviewed branch: task-64.14-tapered-eval
Reviewed implementation: 88b78c0
Verdict: approved

REV-1-01 resolution verified: the TASK-27 depth-6 paired run isolates retained razoring from an otherwise identical razoring-disabled comparator. The archived report records 10 wins, 0 draws, 10 losses, Elo 0.0, LLR 0.0, and zero crashes/forfeits; retaining the current margin is supported as a non-authoritative smoke decision.

Verification:
- git merge-base --is-ancestor aa915d85d32d03d829d0636c6af3e71b40a6632f 88b78c0: pass
- git diff --name-status 88b78c0..138288e: task metadata only
- cargo fmt --check: pass
- clean-CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (43 core; 266 engine passed, 2 ignored; 19 integration; 1 doc test)
- /tmp/seaborg-strength-64_14-rework/artifacts-razoring-depth6/report.json: present and matches recorded 10W-0D-10L result
- perft/movegen benchmark: not verdict evidence because these benches do not exercise static evaluation; another task was also actively benchmarking, so the machine did not meet the required idle condition
---

author: @codex
created: 2026-07-19 21:07
---
Merge integration failure
Primary tip tested: c7826f15b267cd89b0c1c02c97b5294f6ec9bf57
Detached trial merge: dadd71d
Failing command: clean-CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings
Evidence: benches/search.rs:186 still calls Position::material_eval(), which TASK-64.14 replaces with static_eval(); Clippy also reports the Evaluation import unused as a consequence. The benchmark file landed on master after the task's recorded base, so the approved target passed in isolation but does not integrate with the live primary tip.
Expected rework: update the search evaluation benchmark to call and describe static_eval(), verify the benchmark still measures the intended evaluator, then rerun the repository gates. cargo fmt --check passed and cargo test --workspace passed on the trial merge, but strict Clippy is a blocking integration gate. Master was not advanced.
---
<!-- COMMENTS:END -->
