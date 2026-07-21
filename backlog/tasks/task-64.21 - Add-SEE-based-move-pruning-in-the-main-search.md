---
id: TASK-64.21
title: Add SEE-based move pruning in the main search
status: Done
assignee:
  - '@codex'
created_date: '2026-07-21 12:56'
updated_date: '2026-07-21 22:23'
labels:
  - search
  - pruning
  - see
dependencies:
  - TASK-64.9
parent_task_id: TASK-64
priority: medium
ordinal: 128000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deferred half of TASK-64.9. That task set out to add static-exchange pruning in both quiescence and the main search, delivered together. Only the quiescence cuts shipped (a robust +70 Elo gain); the main-search prune was measured to be net-harmful and was removed. This task is to make main-search SEE pruning actually gain, or to conclude with evidence that it cannot in this engine.

WHAT WENT WRONG LAST TIME (TASK-64.9 measurements, fastchess nodes=100000 vs the merge-base, one sitting):
- Quiescence SEE + delta cuts ALONE: +70.4 +/- 39.4 Elo (LOS 99.99%, 200 games). Shipped.
- Main-search prune ALONE (non-PV, shallow depth, depth-scaled SEE floor; captures floor -(300+100*depth), quiets floor -60*depth; check-giving moves exempted): -19.1 +/- 40.3 Elo (LOS 17%, 200 games; not individually significant).
- BOTH cuts together: -88.7 +/- 19.9 Elo (LOS 0%, 500 games).
So the two prunes interact strongly and destructively: q-cuts alone +70, but adding the main-search prune collapses the combined change to -88 (a ~158 Elo negative interaction). The mechanism is NOT understood and was NOT the material-only-eval guess made at the time (the leaf eval is a tapered HCE / NNUE, not material-only). Understanding this interaction is the crux of the task.

TWO HARD CONSTRAINTS THAT SURFACED:
1. The capture floor is pinned near-inert by the shallow forced mates in the search regression suite (gives_correct_answers, child_mate_windows_preserve_distance_parity). A sacrificial mating capture has SEE ~ -300; pruning captures losing less than ~a minor piece reverts those mates to a bare material score. So the capture floor had to be kept a minor piece deep (-(300+100*depth)), which prunes almost nothing. Net: the main-search prune's only real effect was the QUIET-move prune.
2. Raw depth-scaled quiet SEE pruning (fire in all non-PV nodes at depth<=6 with floor -60*depth) is the harmful part and does not compose with the q-cuts.

SUGGESTED DIRECTION (not a committed plan; the worker should research current code first):
- Match Stockfish-style gating rather than a raw depth floor: scale the quiet SEE threshold by lmrDepth (the LMR-reduced depth) not raw depth, and gate on move ordering / history so only late, low-history quiets are cut. Consider capture SEE pruning gated on depth and move count with a floor that still respects the mate suite.
- ALWAYS measure in combination with the shipped quiescence cuts (the current master behaviour), never in isolation, because the interaction is where the loss lives. Use the TASK-27 harness against the appropriate baseline.
- If a properly-gated version still cannot beat the q-cuts-only baseline, close the task with that negative result recorded rather than shipping a regression.

Reference commit for the removed approach and full analysis: the TASK-64.9 implementation notes and its target commit on branch task-64.9-see-pruning.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A main-search SEE prune is added and, combined with the existing quiescence cuts, is measured with the TASK-27 harness to be non-negative versus the quiescence-cuts-only baseline (or the task is closed with a recorded negative result showing it cannot beat that baseline)
- [x] #2 The shallow forced mates in the search regression suite (gives_correct_answers, child_mate_windows_preserve_distance_parity) still pass
- [x] #3 The prune is confined to non-PV nodes, never fires while in check, always searches the first move, and exempts or otherwise protects checking/sacrificial moves so tactics are preserved
- [x] #4 Strength measured in COMBINATION with the quiescence cuts (not in isolation), with the interaction between the two prunes explicitly characterised in the implementation notes
- [x] #5 Quiescence node counts / share and main-search node counts reported before and after on a representative position set
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Research current search: main move loop (search.rs ~1662+), existing q-cuts (see_pruning_enabled), LMR/LMP/futility gating, history table access, see() interface. DONE.
2. Implement a Stockfish-style main-search SEE prune, differing from the removed TASK-64.9 raw-depth version in the ways the task diagnoses as the fix:
   - Quiets: scale the SEE threshold by the LMR-REDUCED depth (lmr_depth = new_depth - lmr_reduction(depth,move_count)), squared, not by raw depth; gate on LOW history (history.get < gate) so only late, unpromising quiets are cut; never fire on move 1.
   - Captures: mate-safe floor -(300 + 100*depth) so sacrificial mating captures (SEE ~ -300) survive; near-inert by design but sound.
   - Guards: non-PV only, never node_in_check, never a move that gives check (decided pre-make, applied post-make like the q-cut), best_value still a cp score, promotions exempt.
   - Reuse the existing see_pruning_enabled()/see_pruning_disabled toggle and the see_skip_node telemetry.
3. Tests: mate suite (gives_correct_answers, child_mate_windows_preserve_distance_parity) still pass; extend see_pruning_leaves_forced_results_unchanged to cover the main prune; add a node-reduction test.
4. Report q-node share and main-search node counts before/after on the representative set.
5. Build baseline (master) and candidate (master + prune) release binaries; measure IN COMBINATION with the shipped q-cuts via fastchess nodes=100000 colour-reversed pairs (baseline = q-cuts-only = current master). Characterise the interaction in the notes.
6. If it cannot beat the q-cuts-only baseline, close with a recorded negative result (AC#1 permits this) rather than shipping a regression.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
IMPLEMENTATION + INVESTIGATION (Step 16b main-search SEE prune)

Built the Stockfish-style properly-gated prune the task's SUGGESTED DIRECTION called for, in the main move loop (search.rs Step 16b), reusing the existing see_pruning toggle plus a new #[cfg(test)] main_see_pruning_disabled isolation toggle:
- Quiet floor scaled by the REDUCED depth: -SEE_PRUNING_QUIET_SLOPE(25) * lmr_depth^2, only when lmr_depth <= SEE_PRUNING_QUIET_MAX_LMR_DEPTH, and only for quiets whose history is non-positive (SEE_PRUNING_QUIET_HISTORY_MAX=0). lmr_depth = new_depth - lmr_reduction(depth,move_count).
- Capture floor mate-safe: -(SEE_PRUNING_CAPTURE_BASE(300) + SEE_PRUNING_CAPTURE_SLOPE(100)*depth) for depth <= SEE_PRUNING_MAX_DEPTH(8). Near-inert by design (keeps SEE~-300 sacrificial mating captures).
- Guards (AC#3): non-PV only, never node_in_check, never move 1, best_value still a cp score (disabled once a mate is in hand), promotions exempt, and the check-giving exemption decided pre-make / applied post-make (SEE reads the pre-move position; the !in_check guard needs the move on the board), mirroring the quiescence cut.

MATE SUITE (AC#2): gives_correct_answers and child_mate_windows_preserve_distance_parity pass with the prune active. Added main_search_see_pruning_leaves_forced_results_unchanged (Qxg7# queen-sac mate, a mate-in-3, and a sac-proof mate-in-7: identical score+move with the main prune on vs off) and main_search_see_pruning_changes_the_search_tree (proves the prune fires).

AC#5 NODE DATA (representative 7-position middlegame set, depth 9, main-prune ON vs OFF with q-cuts on in both), config lmr_depth<=2:
  OFF: main=530725 q=708041 all=1238766 q_share=57.16%
  ON : main=452857 q=578181 all=1031038 q_share=56.08%
  ratios: all=0.832 main=0.853 q=0.817
The node effect is NOT monotone: on r3k2r/pppq1ppp/2np1n2/4p3/2B1P3/2NP1N2/PPP2PPP/R2Q1RK1 at depth 8 the prune GROWS the tree (129966 vs 99574). A static SEE verdict sometimes discards a move that would have produced a beta cutoff (a defended-square quiet, a positional sacrifice), forcing the node to search its whole list. This is the destructive mechanism behind the 64.9 interaction, now understood.

STRENGTH (AC#1/#4) — measured IN COMBINATION with the shipped quiescence cuts; baseline = q-cuts-only master 8089a50 (binary sha256 9bb46d85). fastchess, colour-reversed pairs, openings-v1.epd.

NODE BUDGET (nodes=100000, non-authoritative smoke budget):
- lmr_depth<=2 (binary 2126efc3): -43.66 +/- 31.86 Elo, LOS 0.32%, 400 games (43.75%, W100/L150/D150).
- lmr_depth<=1 (binary dd544fd4): +136.97 +/- 27.20 Elo, LOS 100%, 400 games (68.75%, W200/L50/D150). Reproduced from a clean rebuild (1e2635b7): 69.5% over 100 games.
=> A ~180 Elo swing from a single-ply change in the quiet-prune depth cap. Under a fixed NODE budget the prune's node savings convert to free extra depth, which is disproportionately rewarded at these low depths, so the node-budget numbers are ARTIFACTS and wildly config-sensitive.

TIME CONTROL (tc=2+0.02, concurrency 4 — the authoritative equal-time budget; the TASK-27 harness authoritative mode requires a time control):
- lmr_depth<=1 (1e2635b7) vs base: -17.39 +/- 31.24 Elo, 300 games (47.5%, W91/L106/D103). base recorded 2-3 time-loss forfeits, which HELP the candidate, so the true figure is if anything a touch worse. Statistically neutral-to-slightly-negative: NO GAIN.

CONCLUSION (AC#1 negative branch): the properly-gated main-search SEE prune CANNOT beat the quiescence-cuts-only baseline at a fair (time-controlled) budget. The large positive node-budget result is a depth-conversion artifact; once depth costs clock and the SEE calls add hot-path overhead, the move-selection distortion cancels the benefit. lmr_depth<=2 loses even under the pruning-favorable node budget (-44). No configuration is expected to cross into a clear time-control gain: the technique trades move-selection accuracy for node savings that do not convert to strength here. Recommend NOT shipping; revisit only after the eval/search foundation changes materially (e.g. a trained NNUE). See also the see-pruning-interaction analysis from TASK-64.9.

Committed artifact (lmr_depth<=1) is the reviewable, reproducible implementation; base 8089a50, target 35dc15f. Required checks green on the target: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings (zero warnings), cargo test --workspace (all pass, incl. the mate suite and the new SEE tests).

MECHANISM DIAGNOSTICS (follow-up to characterise WHY, requested after handoff) — these CORRECT the earlier 'prune removes cutoff moves' framing.

Two deterministic experiments (no games):

1. SEE-of-quiet correctness (new test see::tests::see_scores_plain_quiet_moves): a quiet move (piece to an empty square, target None as the prune passes it) scores 0 when safe, negative when the piece hangs, and 0 (not a false negative) when the square is attacked but adequately defended. A quiet can never score positive. So the signal the prune consumes is sound at the source.

2. Cutoff-rate of would-be-pruned moves (ignored diagnostic see_prune_cutoff_rate_diagnostic + a #[cfg(test)] see_diag instrument that suppresses the prune and searches each flagged move instead). Depth 10, 7-position middlegame set, lmr_depth<=1 config:
   - would-be-pruned moves: 48060; of those beta cutoffs 648 = 1.35%; became a new best without cutting off 0 = 0.00%.
   - baseline late (non-first) quiets: 440247; beta cutoffs 17749 = 4.03%.
   => flagged moves cut off ~3x LESS than a typical late quiet. The prune is well-targeted, NOT grossly discarding good moves. The earlier 'removes cutoff moves because ordering is bad' explanation is refuted.

CORRECTED MECHANISM for the no-gain result: the prune is well-targeted but LOW-LEVERAGE in this shallow search. It fires on only ~11% of late quiets, near the horizon where each correctly-pruned subtree is tiny, while the residual ~1.35% missed cutoffs are individually expensive; savings and costs roughly cancel => net within noise of zero (matches TC -17 +/- 31), position-dependent (grows the tree on some, shrinks on others).

WHY STOCKFISH GAINS AND WE DON'T (and it is NOT our SEE): our LMR is nearly a no-op (lmr_reduction returns 1, or 2 only at depth>=8 & move_count>=8), so lmr_depth ~ raw_depth-1 and the 'lmrDepth scaling' is almost inert (lmr_depth<=1 ~ depth<=3; lmr_depth<=2 ~ depth<=4 — which is why one ply swung it so hard). SF's SEE pruning gets its leverage from aggressive LMR (fires on deeply-reduced tail moves), much greater search depth (savings compound over a far larger tree), and co-tuned continuation-history pruning that pre-filters. It is a small term even in SF. Our search is too shallow / our LMR too timid for it to clear the noise. Revisit AFTER LMR is strengthened (and/or NNUE lands), not now.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-21 21:02
---
Needs-human handoff: negative result recorded, do NOT merge

Branch: task-64.21-main-search-see-pruning
Worktree: /Users/seabo/seaborg-worktrees/task-64.21-main-search-see-pruning
Base: 8089a508d616283bd17a50a8ccffcc444f76857b (master, q-cuts-only baseline)
Implementation target (artifact): 35dc15f
Verification (on target):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (zero warnings)
- cargo test --workspace: pass (incl. mate suite + new SEE tests)
Known failures: none

DECISION NEEDED. AC#1 offers two outcomes: a main-search SEE prune that is non-negative vs the q-cuts-only baseline, OR a recorded negative result showing it cannot beat that baseline. I built the properly-gated Stockfish-style prune the task asked for and measured it thoroughly IN COMBINATION with the shipped quiescence cuts:
- Node budget (nodes=100000, non-authoritative): lmr_depth<=1 = +137 Elo, lmr_depth<=2 = -44 Elo (a ~180 Elo swing from one constant => the node budget rewards saved nodes as free depth; these are artifacts).
- Time control (tc=2+0.02, the authoritative equal-time budget): lmr_depth<=1 = -17.4 +/- 31.2 Elo, 300 games => NEUTRAL-TO-SLIGHTLY-NEGATIVE, no gain (and base's 2-3 time-forfeits flatter the candidate).
=> AC#1's NEGATIVE branch is satisfied: the prune cannot beat the baseline at a fair budget. Full data + mechanism in the implementation notes.

RECOMMENDATION: do not ship. The committed artifact adds hot-path SEE overhead for no strength gain, and the merge gate runs only perft/NPS benchmarks, not strength tests, so it would not catch this. I have therefore NOT moved this toward Ready to Merge. As the implementation agent I cannot move to Done or create a follow-up. A human should either: (1) ratify closing this task as a recorded negative result (the artifact stays in branch history for reference; the branch is not merged as a feature), or (2) direct a different approach. Per the lifecycle I leave that scope/terminal decision to you. The worktree is clean.
---

author: @codex
created: 2026-07-21 22:17
---
Mechanism now fully characterised (AC#4). Updated handoff; recommendation unchanged: do NOT ship.

Base: 8089a50 | Implementation target: 2353acb (adds mechanism diagnostics on top of the 35dc15f prune) | Worktree clean.
Checks re-run green on the target: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings (zero warnings), cargo test --workspace (all pass, incl. new SEE-of-quiet validation).

Two deterministic diagnostics (requested after the first handoff) CORRECT my earlier 'removes cutoff moves' framing:
- SEE-of-quiet is correct (0 safe / negative when hanging / 0 when defended).
- With the prune suppressed and each flagged move searched instead: flagged moves cut off at 1.35% vs a 4.03% late-quiet baseline, 0% become a new best. The prune is WELL-TARGETED, not discarding good moves.
The no-gain result is because the prune is LOW-LEVERAGE in this shallow search (fires on ~11% of late quiets near the horizon where saved subtrees are tiny; the residual ~1.35% missed cutoffs are expensive; savings ~ costs -> net within noise of zero, matching the TC -17 +/- 31). It gains in Stockfish because SF has aggressive LMR (so the same idea fires on deeply-reduced tail moves), far greater depth, and co-tuned continuation-history pruning — leverage our search lacks; our LMR is nearly a no-op so lmr_depth ~ raw_depth and the scaling is inert.

So it is NOT a fundamental 'cannot work' — it is 'no measurable gain at our current search strength; the +137 node-budget figure was a depth-conversion artifact'. Recommend closing as a negative result now and revisiting after LMR is strengthened / NNUE lands. As implementation agent I cannot move to Done or file a follow-up; leaving the terminal/scope decision to you.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
NEGATIVE RESULT (human-ratified closure; AC#1's negative-result branch). Implemented a properly-gated Stockfish-style main-search SEE prune — quiet floor scaled by the reduced (lmr) depth with a non-positive-history gate, a mate-safe capture floor, confined to non-PV nodes, never in check, never the first move, promotions and check-giving moves exempt, disabled once a mate is in hand — and measured it IN COMBINATION with the shipped quiescence cuts (baseline = q-cuts-only master 8089a50).

It does not gain at a fair budget: tc=2+0.02 gave -17 +/- 31 Elo over 300 games (no measurable gain). The +136.97 Elo at nodes=100000 was a depth-conversion artifact of the node budget (lmr_depth<=2 was even -44 under that pruning-favorable budget). Deterministic diagnostics show the prune is well-targeted, not mis-firing: SEE-of-quiet is validated correct (0 safe / negative when hanging / 0 when defended), and with the prune suppressed the flagged quiets cut off at 1.35% vs a 4.03% late-quiet baseline (0% become a new best). It is simply low-leverage in this shallow search — the node savings roughly equal the cost of the residual ~1.35% missed cutoffs. Root cause: our LMR is nearly a no-op (lmr_reduction returns 1, or 2 only at depth>=8 & move_count>=8), so lmr_depth ~ raw depth and the lmrDepth scaling is inert; Stockfish's gain comes from aggressive LMR, far greater depth, and continuation-history pre-filtering.

Not shipped: master engine behavior is unchanged. The implementation and the mechanism diagnostics remain in branch task-64.21-main-search-see-pruning (target 2353acb, base 8089a50) for reference. Flagged on TASK-64.22 (LMR refinement) to revisit after LMR is strengthened. Verified on the target: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings (zero warnings), cargo test --workspace (all pass, incl. the mate suite and the new SEE tests).
<!-- SECTION:FINAL_SUMMARY:END -->
