---
id: TASK-86.5
title: Run the NNUE architecture sweep on the corpus and select the v2 network
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-25 12:24'
updated_date: '2026-07-29 06:58'
labels:
  - nnue
dependencies:
  - TASK-86.3
  - TASK-86.4
  - TASK-81
  - TASK-86.7
  - TASK-86.8
parent_task_id: TASK-86
priority: high
ordinal: 147000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Execute the architecture sweep defined by the methodology decision doc (TASK-86.3) against the fixed corpus from TASK-81, and select the network to promote. Sweep the axes that matter, roughly one factor at a time so results are attributable: feature-transformer width H (e.g. 256 -> 512 -> 1024 -> 2048), activation (CReLU vs SCReLU), output-stack depth and output buckets (none vs 8), and dense-tail quantization. For each candidate record post-QAT quantized validation loss and realized in-engine single-thread NPS, plot the loss/NPS Pareto frontier, and run fixed-time-control SPRT on the frontier finalists against the current gen-002 default (and against each other). Output a selected network plus a report that also reads off whether the corpus is label-limited or capacity-limited at the chosen size.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A loss-vs-NPS Pareto frontier is produced over the swept architectures following the TASK-86.3 protocol, with the swept factors and any coverage limits recorded
- [ ] #2 Frontier finalists are evaluated by fixed-time-control SPRT against the gen-002 default, with results and attribution recorded in BENCHMARKS.md
- [ ] #3 A single network is selected with a written rationale grounded in fixed-TC Elo, and the report states whether the corpus is label-limited or capacity-limited at that size
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Resumed after TASK-86.8 (parallel dataloader) merged. Phase 1 (screen) on the rig:
1. Sync rig clone to this branch (master + 86.8 merged + width axis extended to 1024); rebuild target-cpu=native release.
2. Run tools/trainer/sweep.py --device cuda --num-workers 8 over corpus-gen-002 (14 candidates, one factor at a time, fixed-everything-but-architecture; by-shard leak-free split). Records post-QAT quantized val loss + in-engine single-thread NPS per candidate, computes loss/NPS Pareto frontier, selects finalists, emits strength_test.py SPRT commands. Monitor candidate-1 epoch time for a firm ETA.
3. Install fastchess on the rig (phase-2 prereq).
4. Commit the frontier report + finalists under this branch; record swept factors and coverage limits (width capped at 1024; AC#1). PAUSE for go/no-go before the multi-day SPRT.
Phase 2 (after approval): finalist SPRT vs gen-002 (+head-to-heads); record results/attribution in BENCHMARKS.md (AC#2).
Phase 3: select the net by fixed-TC Elo; write rationale + label-limited vs capacity-limited read; promote (AC#3).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Paused pending TASK-86.8 (parallelize the packed dataloader). Phase-0 rig setup done and preserved on this branch: rig clone synced to this branch, target-cpu=native release built, by-shard split pre-flighted (92.8M train / 10.3M val, shards 000+018 held out, deterministic), sweep width axis extended to 1024 (14 candidates). Measured per-epoch ~2.5-3.2 min, GPU ~19% (single-thread dataloader is the bottleneck), so the 14-candidate screen is an overnight job at the default budget; parallelizing the loader first (86.8) makes it ~3-4h. Resume here after 86.8 lands: rebuild engine, run tools/trainer/sweep.py on corpus-gen-002.

Phase-1 screen launched on the rig (2026-07-28T09:16Z). Engine commit 6793c34 (target-cpu=native release). sweep.py: 14 candidates, --device cuda --num-workers 8 --epochs 30 --batch-size 8192 --lambda 0.3 --scale 400, by-shard split (seed 0), finalists 3, elo0/elo1 0/5 tc=10+0.1. Corpus corpus-gen-002 (103,086,342 records; 92.8M train / 10.3M val, shards 000+018 held out). Out-dir ~/rl/sweep-86.5. Measured parallel per-epoch ~2 min (GPU ~73%); ETA ~15h. Parallel-loader equivalence confirmed on real data (epoch1/2 losses byte-identical to serial). fastchess NOT yet installed (phase-2 prereq; deferred so NPS measurements stay uncontended).

Bucket-track finding + planned diagnostic (analysis, not yet run):
The loss/NPS screen fairly ranks the width axis (all widths share the mature feature-transformer + single-output path) but CANNOT fairly rank the v2 features (output stack / buckets / SCReLU). Two own-engine biases:
- Loss: bucketed nets are undertrained under the fixed 30-epoch budget. Per-epoch val-loss tails (from the .pt history) show buckets still descending at epoch 30 (b4 drop_last5 0.51%, b1 0.28%) while every non-bucketed net is flat (<=0.13%). Buckets split data per piece-count bin so each bucket's stack converges slower; fixed LR (1e-2) is tuned for the shallow baseline. The 'buckets' axis also conflates buckets with the output-stack addition (b1 = stack+1bucket already regresses vs baseline).
- NPS: the output layer runs fresh per node (FT is incremental), so output-path overhead is amplified. Measured per-node cost jump far exceeds the added arithmetic: +0.57 us/node for a tiny output stack (b1), +1.75 us/node for SCReLU (256 square-and-clamps). AVX2 kernels exist but the fresh-per-node output path (per-layer dequant/activation/bucket-select, slower dot_screlu) over-charges the v2 features. So the cost axis reflects inference immaturity, not architectural cost.

Decision (user): quantify the training half now. After the screen completes, retrain the stack-only (buckets__b1) and canonical bucketed (buckets__b8) candidates from scratch at 2-3x epochs (target 90) on the same by-shard split, same config; compare converged val loss to the baseline (already flat-tailed at 30, so a fair converged reference). If the gap collapses, the screen's bucket loss was a convergence artifact; if it barely moves, buckets are genuinely weaker on this corpus. The inference-cost half (profile/optimize the output-stack + SCReLU path) is deferred to a possible follow-up. This bucket-convergence result feeds AC#1 coverage limits and the AC#3 label-vs-capacity read; the raw screen must NOT be read as 'buckets dominated'.

Train/val-gap diagnosis (from checkpoint histories; no data-scaling in the sweep -- every candidate gets the full 92.8M-record train set and 30 epochs regardless of parameter count).
Gap = (val-train)/val at epoch 30:
- Width axis is LABEL/DATA-limited at the top: gap climbs monotonically 1.19% (h128), 2.62% (h256), 3.89% (h384), 4.84% (h512), 7.41% (h1024). Val loss flattens h512->h1024 (0.011119->0.011085) while train keeps dropping (0.010581->0.010264): more width fits train labels but not val. => beyond ~h512 the lever is better labels (datagen node budget), not more params. This is the AC#3 capacity-vs-label read for width.
- Buckets are NOT data-limited -- they UNDERFIT. b8 train loss (0.011633) is higher than baseline train (0.010992), with a normal gap (b1 2.40%, b4 1.86%, b8 2.60%, <= baseline 2.62%). A data-limited net would show low train + large gap; instead both losses are elevated => epoch/optimization-limited (slow per-bucket convergence under the fixed budget). This validates the higher-epoch/LR retrain as the correct diagnostic for buckets; data volume is not the bucket bottleneck.

Why width saturates early (~h512) vs much wider frontier nets -- own-data reasoning, not a methodology bug:
The usable width is capped by our corpus, and our own train/val gap shows it: h1024 has the LOWEST train loss (0.010264) of all candidates but a 7.4% train/val gap with flat val loss (h512->h1024 val 0.011119->0.011085). Width IS extracting more from the training labels; it just stops generalizing -- a data/label ceiling, not underfitting or a broken FT (h1024 optimizes fine, unlike the buckets). Root causes: (1) corpus is ~93M train positions vs the tens of billions large frontier nets train on -> far fewer examples per parameter; (2) labels are gen-002 self-play search scores at the TASK-81 datagen node budget (early generation, modest sharpness) -> limited information ceiling; (3) single-generation fixed corpus vs many-generation data/net co-evolution. Implication (label-limited branch, AC#3): width is not fundamentally capped at ~512 -- it is capped by our data. Unlocking wider nets requires more/sharper labels (datagen node budget + more positions + more generations), then a width re-sweep; it is a datagen/RL investment, not a width knob in this sweep. For THIS decision: pick the best width on the current corpus (~h512 pending SPRT); treat 'go wider' as gated on a label investment.

Screen COMPLETE (14/14, SWEEP_EXIT=0). Frontier (4): width h128, baseline h256, width h512, width h1024 -- the entire frontier is the plain-width CReLU axis. Auto-finalists (3): h1024/h256/h128 -- the even-NPS sampling SKIPPED h512 (the loss/NPS knee); h512 should be added to the phase-2 SPRT set. gen-002 (default.sbnn) NPS reference = 716,725 (next to the retrained h256 baseline, same architecture). Full screen table + reading committed to artifacts/sweep-86.5/RESULTS.md and sweep.json. Follow-ups launched: baseline/b1/b8 retrain at 60 epochs (bucket-undertraining quantification); gen-002-vs-new-corpus loss decomposition at fixed h256 still to build. Phase-2 SPRT (multi-day) awaiting go/no-go.

Bucket retrain (60 epochs, same config) RESULT -- refutes the epochs-undertraining hypothesis:
val@30 -> val@60: baseline 0.011288 -> 0.011303 (flat, confirms converged); b1 0.011429 -> 0.011429 (ZERO change, already converged); b8 0.011944 -> 0.011871 (-0.6%, still ~5% above baseline). Doubling epochs did NOT close the bucket gap. So the earlier tail-slope read of 'undertraining' was misleading; more epochs is not the fix.
BUT not an architecture verdict either: at 60 epochs b8 TRAIN loss (0.011603) is still higher than baseline TRAIN (0.010969) despite b8 having more capacity. A higher-capacity net underfitting even the train set => the training RECIPE is not exploiting the capacity (candidate causes: fixed LR 1e-2 wrong for the deeper stack; 256->16 bottleneck choking gradient flow; per-bucket routing weakening signal). Indicated next lever = training-recipe investigation (LR/schedule/init) for the deeper bucketed heads -- a proper follow-up task, NOT more epochs and NOT abandoning buckets. Screen still cannot fairly rank the v2 features until (a) this recipe issue and (b) the inference-cost overhead are addressed.
<!-- SECTION:NOTES:END -->
