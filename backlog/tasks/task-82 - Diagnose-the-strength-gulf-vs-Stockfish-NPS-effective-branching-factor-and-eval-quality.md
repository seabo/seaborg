---
id: TASK-82
title: >-
  Diagnose the strength gulf vs Stockfish: NPS, effective branching factor, and
  eval quality
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-24 11:00'
updated_date: '2026-07-25 16:53'
labels:
  - search
  - eval
  - benchmark
  - investigation
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/eval.rs
  - BENCHMARKS.md
priority: high
type: spike
ordinal: 138000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
We are roughly 1000-1500 Elo behind the frontier and do not know how that gap decomposes. Playing strength factors into three axes; measure each against Stockfish on the same single-core hardware so effort can be allocated between a bigger NNUE and a faster/deeper search. Output is a report and a recommendation, not an engine change. Fully orthogonal - runs alongside other work. Using Stockfish only as a measurement reference is fine and does NOT touch the TASK-69 self-play-purity constraint (that governs training data, not benchmarking).

The three axes and how to measure each (note the corrected physics: selectivity buys depth-per-node, so it shows up on the fixed-time/fixed-node axis, NOT at fixed depth):

1. NPS (raw speed). Run 'go depth N' / bench on a fixed position suite, single thread, and record nodes/sec for seaborg and Stockfish. Our net is small, so our raw NPS may exceed SF's even though each SF node is worth more - which is exactly why NPS alone is not conclusive.

2. Selectivity / effective branching factor (depth-per-node). From the same runs, EBF ~= nodes^(1/depth); also record nodes-to-reach-depth-N and depth-reached-at-fixed-time (the latter combines NPS x EBF). Compare to SF. This is where reductions/pruning quality shows up.

3. Eval quality (per-position accuracy), isolated from search. Cleanest: a search-free static-eval agreement test - take a reference-labelled position set (label via deep Stockfish search or a public suite) and measure how well each engine's STATIC eval ranks the positions. Compare on a scale-independent metric (rank/Spearman correlation, or winner-prediction accuracy on decisive positions, or after mapping cp to a WDL logistic) - never raw centipawns, the scales differ. Optionally corroborate with a fixed-depth match with forward pruning/reductions/extensions disabled on both sides (approximate, since SF cannot be fully neutered).

Bounding the decomposition: also run short head-to-head gauntlets seaborg-vs-SF at fixed NODES and at fixed TIME. The fixed-nodes gap removes the NPS axis, so the residual is eval + selectivity-per-node; comparing to the fixed-time gap shows how much raw speed is worth.

Minimal tooling permitted within scope: a UCI helper to dump seaborg's static eval for a FEN if one does not already exist; small scripts to drive the runs. No hot-path change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 NPS for seaborg and Stockfish is measured on a fixed single-core suite and recorded, with the compile flags (AVX2), hardware, and SF version noted
- [x] #2 Effective branching factor is reported for both engines (nodes^(1/depth) and/or nodes-to-fixed-depth), alongside depth-reached-at-fixed-time
- [x] #3 Eval quality is isolated with a search-free static-eval agreement test against a reference-labelled position set, reported on a scale-independent metric, not raw centipawns
- [x] #4 Head-to-head Elo gaps at fixed nodes and at fixed time are measured to bound how much of the gulf is speed versus eval-plus-selectivity
- [x] #5 A written attribution decomposes the gulf into eval vs NPS vs selectivity and recommends where to invest (larger NNUE vs search speed/selectivity), with methodology recorded for reproducibility
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a search-free static-eval UCI dump to seaborg (the one permitted tooling change): an 'eval' command that prints the NNUE forward-pass score (and hand-crafted static_eval) for the current position, side-to-move perspective. No hot-path change.
2. Provision the measurement rig (AMD Ryzen 9 3900XT, AVX2, idle): build seaborg release with target-cpu=native from the task branch; use the vendored stockfish-ubuntu-x86-64-avx2 and ~/.local/bin/fastchess. Record SF version, flags, hardware. Measure only when the machine is idle.
3. AC#1 NPS: drive UCI 'go depth N' single-thread over a fixed FEN suite for seaborg and Stockfish; record nodes, nps.
4. AC#2 EBF: from the same runs compute nodes^(1/depth), nodes-to-fixed-depth, and depth-reached-at-fixed-time for both engines.
5. AC#3 Eval quality: label a position set with deep Stockfish search; dump seaborg static eval (NNUE) per FEN; compare on a scale-independent metric (Spearman rank / WDL-mapped accuracy), never raw cp.
6. AC#4 Head-to-head: fastchess seaborg-vs-Stockfish at fixed NODES and at fixed TIME; use an SF handicap ladder (reduced nodes/time) to find parity points so the gap is measurable, isolating NPS (fixed-time vs fixed-nodes).
7. AC#5: write the attribution (eval vs NPS vs selectivity) and the invest-recommendation into BENCHMARKS.md/report with full methodology for reproducibility. Small driver scripts committed under tools/. No engine hot-path change.
8. Run required checks (fmt/clippy/test), commit, hand off to review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Measurement campaign (local Apple M3 Pro, 12c; Stockfish 18 arm64; seaborg native ARM release, scalar NNUE — no ARM NEON path, so NPS is a pessimistic bound vs seaborg's x86 AVX2 deployment). Findings so far:
- NPS (d=14, 20-pos suite, best-of-3): seaborg 642k agg / 687k median vs SF 727k agg / 849k median. Comparable despite scalar handicap -> speed is not the bottleneck.
- Selectivity/EBF: seaborg needs ~15x more nodes to reach depth 14 (374k vs 24k); at fixed 1500ms SF reaches median depth 22 vs seaborg 14 (8-ply gap). EBF 2.42 vs 2.00.
- Eval agreement (500 gen'd positions, deep-SF depth-20 labels, 446 decisive): seaborg static Spearman 0.931 / winner-acc 0.946 vs SF static 0.954 / 0.975. Eval close but modestly behind.
- Fixed-nodes gauntlet: at equal nodes SF wins ~100%; parity sweep underway to quantify the node ratio (early estimate ~40-50x).

Final measurement summary (Apple M3 Pro, 12c; Stockfish 18 arm64; seaborg scalar-NNUE ARM release — NPS is a pessimistic bound, node/eval axes exact):
- NPS ~comparable (seaborg 642k agg vs SF 727k). Speed is not the bottleneck.
- Selectivity dominant: ~15x nodes to reach depth 14 (374k vs 24k); 8 plies shallower at 1500ms (median 14 vs 22); ~40-50x nodes for parity strength (seaborg 100k ~= SF ~2-2.5k).
- Eval near-parity: static-eval Spearman 0.931 vs 0.954 (winner-acc 94.6% vs 97.5%) over 500 deep-labelled positions.
- Head-to-head floors: equal-nodes 0/20 and equal-time 0/40 (both ~100% loss); identical whether budget is nodes or time confirms NPS contributes nothing. Fixed-TIME parity sweep intentionally not reported: SF parity budget is single-digit ms, dominated by move-overhead + shared-host jitter (spurious 92% at one rung).
Recommendation: invest in search selectivity (reductions/pruning/extensions/ordering), not a larger NNUE. Report + tables + methodology in BENCHMARKS.md; tooling in tools/diag/.
Engine change: search-free 'eval' UCI command (staticeval cp <v>) reusing the search leaf evaluator. No hot-path change.
Note: brew-installed Stockfish 18 locally as the reference (system change, benign).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-24 22:11
---
Implementation handoff
Branch: task-82-diagnose-strength-gulf-vs-stockfish
Worktree: /Users/seabo/seaborg-worktrees/task-82-diagnose-strength-gulf-vs-stockfish
Base: b2f945778c1ea3a02018c91d00ab4e5923f7d450
Implementation target: 8f83bdc715b8d3697e864e419bd2186c322eedff
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (428 engine lib tests + workspace; new tests uci::parses_eval and search::static_eval_reports_the_hand_crafted_leaf_from_the_side_to_moves_view pass)
Known failures: none. During an early full-suite run under heavy machine load, the pre-existing timing test search::tests::an_extendable_budget_is_still_bounded_by_its_hard_half flaked once (ran 202ms vs 60ms hard limit); it passes in isolation and in the final full run. This task makes no time-management change, so it is not implicated.
Notes for reviewer: measured on Apple M3 Pro (ARM), so AC#1's literal AVX2 build does not apply; seaborg runs scalar NNUE on ARM (no NEON path), which understates its speed vs its x86 AVX2 deployment. This is documented in BENCHMARKS.md and only strengthens the 'speed is not the bottleneck' finding. The node-based selectivity and eval-agreement axes are ISA-independent and exact.
---

author: @george
created: 2026-07-25 16:31
---
Rework: corrected the attribution/recommendation
Branch: task-82-diagnose-strength-gulf-vs-stockfish
Worktree: /Users/seabo/seaborg-worktrees/task-82-diagnose-strength-gulf-vs-stockfish
Base: b2f945778c1ea3a02018c91d00ab4e5923f7d450
Implementation target: ee2ff691e7af083512081338102685b292580ced
Change: BENCHMARKS.md only. The original write-up concluded 'invest in search selectivity, not a larger network / eval is a minor factor'. That was overstated and is corrected: (1) the eval-agreement metrics saturate — seaborg's ~2x winner-error-rate on decisive positions (5.4% vs 2.5%) is not 'near parity', and there is no rho->Elo calibration; (2) eval and selectivity are coupled — a better eval improves ordering and enables safer/deeper pruning, so part of the measured selectivity deficit may be eval-limited; (3) the engine's own +337 Elo jump from PST to the current small net argues the network is far from its ceiling. Reframed: raw speed is ruled out (firm); eval and selectivity are both high-leverage and this diagnostic does not rank them; two follow-up experiments proposed to price the eval headroom directly.
No code change since the prior target 8f83bdc — engine and tooling are byte-identical; only the report prose changed.
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (docs-only change; code identical to the prior green run). The pre-existing load-sensitive timing test search::tests::an_extendable_budget_is_still_bounded_by_its_hard_half flaked once under heavy host load and passes in isolation and on rerun; not implicated by this task.
Known failures: none.
---

author: @george
created: 2026-07-25 16:52
---
Review attempt: 1
Reviewed branch: task-82-diagnose-strength-gulf-vs-stockfish
Reviewed implementation: ee2ff691e7af083512081338102685b292580ced
Base: b2f945778c1ea3a02018c91d00ab4e5923f7d450
Verdict: approved

Immutable target confirmed: base is an ancestor of the target, the target is an ancestor of the branch tip (4542b26), and the only commit after the target changes just the task file (re-handoff metadata) — no implementation file differs between ee2ff69 and the tip.

Acceptance criteria (all proven):
- AC#1 NPS measured single-core over a fixed 20-position suite and recorded with hardware/SF version. Deviation noted and accepted: measured on Apple M3 Pro with the scalar-NNUE aarch64 release (no NEON path) rather than an x86 AVX2 build. This is a pessimistic bound on seaborg's speed and only strengthens the 'speed is not the bottleneck' conclusion; the report flags a confirmatory x86 AVX2 pass as optional tightening that cannot change the conclusion. Environment (flags, hardware, SF 18 arm64) is recorded.
- AC#2 EBF reported for both engines: geomean nodes^(1/depth) (2.42 vs 2.00), mean nodes-to-depth-14 (373553 vs 24491), and depth-at-1500ms (median 14 vs 22).
- AC#3 Search-free static-eval agreement over 500 deep-SF-labelled positions on scale-independent metrics (Spearman + decisive-winner accuracy), not raw cp.
- AC#4 Head-to-head at fixed nodes (0/20) and fixed time (0/40) plus a fixed-node parity sweep (~40-50x); the fixed-time parity sweep is correctly reported as unreliable (collapses into move-overhead/jitter) rather than fabricated.
- AC#5 Written attribution (eval vs NPS vs selectivity) with recommendation and reproducible methodology in BENCHMARKS.md + tools/diag/.

Engine change reviewed: SearchEngine::static_eval mirrors Search::evaluate exactly (network branch: forward pass, no flip; hand-crafted branch: White-perspective score * pov). The Eval command is intercepted in the driver loop that owns the evaluator. Comments are self-contained and explain the reasoning. No hot-path modification (evaluate/movegen unchanged), so hot-path benchmarks are not implicated. No new #[allow] introduced.

Verification (run on the target, fresh CARGO_TARGET_DIR):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: 427 passed, 2 ignored, 1 failure = search::tests::an_extendable_budget_is_still_bounded_by_its_hard_half, which is untouched by the diff, is a documented load-sensitive timing flake (full run was under heavy load, perft tests >60s), and passes in isolation. Not patch-introduced.
- eval command end-to-end: startpos -> staticeval cp 12; r5k1/8/8/8/8/8/8/6K1 b -> staticeval cp 1085 (correct side-to-move sign).

Approved. Code target: ee2ff691e7af083512081338102685b292580ced.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Spike: decomposed the ~1000-1500 Elo gulf vs Stockfish into NPS, selectivity, and eval axes. Delivered a search-free 'eval' UCI command (staticeval cp <v>, side-to-move view, reusing the search leaf evaluator with no hot-path change), a reproducible diagnostic harness under tools/diag/, and a written attribution in BENCHMARKS.md. Findings (Apple M3 Pro, scalar-NNUE ARM release = pessimistic speed bound; SF 18 arm64): NPS comparable (642k vs 727k) so raw speed is ruled out; selectivity far behind (~15x nodes to depth 14, 8 plies shallower at 1500ms, ~40-50x nodes for parity); static-eval agreement Spearman 0.931 vs 0.954 (winner-acc 94.6% vs 97.5%, ~2x error rate on decisive positions). The rework correctly reframed the conclusion: speed ruled out firmly, but eval and selectivity are coupled and this diagnostic cannot rank them, so neither is deprioritised. Verified: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -D warnings clean (fresh CARGO_TARGET_DIR); cargo test --workspace 427 pass / 2 ignored (the sole failure, an_extendable_budget_is_still_bounded_by_its_hard_half, is a pre-existing load-sensitive timing flake untouched by the diff and passing in isolation); the eval command exercised end-to-end (startpos->cp 12, black-up-a-rook->cp 1085, correct sign).
<!-- SECTION:FINAL_SUMMARY:END -->
