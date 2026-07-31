---
id: TASK-86.5
title: Run the NNUE architecture sweep on the corpus and select the v2 network
status: Done
assignee:
  - '@george'
created_date: '2026-07-25 12:24'
updated_date: '2026-07-31 17:20'
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
- [x] #1 A loss-vs-NPS Pareto frontier is produced over the swept architectures following the TASK-86.3 protocol, with the swept factors and any coverage limits recorded
- [x] #2 Frontier finalists are evaluated by fixed-time-control SPRT against the gen-002 default, with results and attribution recorded in BENCHMARKS.md
- [x] #3 A single network is selected with a written rationale grounded in fixed-TC Elo, and the report states whether the corpus is label-limited or capacity-limited at that size
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

gen-002-vs-new-corpus loss decomposition (loss_decomp.py; reconstruction reproduces the screen's 0.011288 exactly -> forward validated). All on the new by-shard val split, fixed h256 architecture: gen-002 (old bootstrap data) 0.012378; retrained h256 (new corpus) 0.011288; h512 (new corpus) 0.011119. Decomposition of the gen3 gain: corpus+recipe -8.8% (dominant), width h256->h512 -1.5% (secondary), total -10.2%. Answers 'was the corpus work worth it': yes, the corpus upgrade is the big lever. Caveat: loss proxy (distribution shift in gen-002's labels); SPRT is the strength arbiter.

Phase-2 SPRT LAUNCHED (2026-07-29T07:57Z). fastchess alpha 1.7.0 (pre-installed ~/.local/bin; matches parser); smoke test passed. Slate: h512, h256, h1024, h128 vs gen-002, run sequentially, concurrency 11, tc=10+0.1, improvement bounds elo0=0/elo1=5, alpha=beta=0.05, max 40000 games each. Both sides are the same commit-6793c34 binary with per-side EvalFile (gen-002 default.sbnn vs the sweep net). Outputs under ~/rl/sweep-86.5/sprt/<name>. h256 (same architecture as gen-002) isolates the corpus Elo; h512 is the leading gen3 candidate. Expect decisive fast PASSes given the ~9-10% held-out loss edge; h1024 more marginal (NPS 434k vs gen-002 717k). Awaiting verdicts.

Campaign complete; report finalized. AC coverage:
- AC#1: loss/NPS Pareto frontier over 14 one-factor-at-a-time candidates (artifacts/sweep-86.5/sweep.json + RESULTS.md), swept factors and coverage limits (width cap 1024, fixed 30-epoch budget, single machine/build) recorded.
- AC#2: four frontier finalists played fixed-TC SPRT vs gen-002; results + full attribution in BENCHMARKS.md (h256 +25.9, h512 +28.0 PASS; h1024 -100.5, h128 -26.8 FAIL).
- AC#3: selected h256 (gen-002 architecture retrained on the new corpus) with a fixed-TC-Elo rationale (h256~h512 tied, width buys no measurable Elo, corpus is the whole gain); report states the corpus is LABEL-LIMITED, not capacity-limited, at this size (next lever = better labels/datagen, not more parameters).
Extras beyond ACs, committed as evidence: gen-002-vs-new-corpus loss decomposition (loss_decomp.py; corpus -8.8% vs width -1.5%), bucket 60-epoch retrain (epochs not the cause of the bucket gap), and the train/val-gap capacity-vs-label diagnosis.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-29 20:50
---
Implementation handoff
Branch: task-86.5-run-nnue-arch-sweep
Worktree: /Users/seabo/seaborg-worktrees/task-86.5-run-nnue-arch-sweep
Base: aff1c39 (merge base with master; where TASK-86.8 merged)
Implementation target: 8099336
Resolved findings: none
Verification:
- No Rust changed on-branch (git diff base..target -- '*.rs' is empty); the cargo fmt/clippy/test gate is the reviewed-green merge base. On-branch code is a 7-line sweep.py width-axis extension (H=1024), covered by test_sweep: python -m unittest test_sweep -> 26 passed.
- Empirical campaign ran on the rig (AMD Ryzen 9 3900XT): 14-candidate screen + 4-finalist SPRT vs gen-002, tc=10+0.1, fastchess alpha 1.7.0, engine git:6793c34 target-cpu=native.
- On-rig evidence (not reproducible in review, per the rig-campaign pattern): ~/rl/sweep-86.5/{nets,checkpoints,sprt/<name>/report.json,*.log}. Committed in-repo: artifacts/sweep-86.5/{RESULTS.md,sweep.json,loss_decomp.py} and the BENCHMARKS.md strength entry.
Known failures: none
Reviewer note: the SPRT/screen numbers are on-rig empirical results; verify the committed report, the reasoning, and the sweep.py tooling (test_sweep), not the un-reproducible campaign.
---

author: @george
created: 2026-07-30 08:29
---
Review verdict: APPROVED — Ready to Merge.

Code target (immutable): 8099336. Base: aff1c39. Branch tip: a555fa3 (post-target commit is task-file/handoff only; git diff 8099336..a555fa3 touches only the task md).

Scope: report + tooling task; no engine behaviour change (git diff aff1c39..8099336 -- '*.rs' empty; engine/ and default.sbnn untouched). Net promotion (baking gen-003 default) is correctly deferred to a follow-up.

Required checks re-run on target 8099336:
- cargo fmt --check: clean.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- cargo test --workspace: all pass (10x 'test result: ok', 0 failed).
- python -m unittest test_sweep (covers the 7-line sweep.py width-axis extension to H=1024): 26 passed.
- No hot-path (.rs) change → perft/movegen bench not applicable.

AC#1 (frontier + factors/limits, TASK-86.3 protocol): artifacts/sweep-86.5/sweep.json is byte-content-identical to the rig's ~/rl/sweep-86.5/sweep.json; the non-dominated frontier {h128,h256,h512,h1024} independently re-derives as correct (h384 rightly excluded — dominated by h512); swept factors (width/activation/buckets/stack/tail-quant) and coverage limits (H=1024 cap, fixed 30-epoch budget, single corpus/machine/build) recorded in RESULTS.md; conforms to docs/nnue-architecture-sweep.md funnel. PROVEN.

AC#2 (finalist SPRT vs gen-002 recorded in BENCHMARKS.md): independently corroborated against on-rig report.json/logs (rig reachable this session, so verified rather than trusted) — h256 852-690-686 +25.9 PASS (2228), h512 794-802-616 +28.0 PASS (2212), h1024 130-197-309 -100.5 FAIL (636), h128 507-954-671 -26.8 FAIL (2132); W-D-L sums match game counts; both sides identical binary sha256=48b39241 with per-side EvalFile (gen-002 default.sbnn vs sweep net), commit 6793c34 target-cpu=native, fastchess alpha 1.7.0, tc=10+0.1, elo0=0/elo1=5/α=β=0.05. All match BENCHMARKS.md exactly. PROVEN.

AC#3 (single net selected, fixed-TC-Elo rationale, label/capacity read): h256 selected — h256 and h512 statistically indistinguishable (+25.9 vs +28.0, overlapping intervals), width buys no measurable Elo, entire ~+26 gain is the corpus; report states LABEL-LIMITED (train/val gap widens 1.2%→7.4% across h128→h1024 with flat val loss past ~h512; next lever = better labels/datagen). Consistent with the committed loss decomposition (corpus -8.8% vs width -1.5%). PROVEN.

Comment quality: the sweep.py comment is self-contained (explains the width-cap reasoning without external references). No undocumented #[allow] introduced. No scope creep.

Verification commands: git merge-base --is-ancestor 8099336 a555fa3; cargo fmt --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace; (cd tools/trainer && python -m unittest test_sweep); rig cross-check of ~/rl/sweep-86.5/{sweep.json,sprt/*/report.json}.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Approved (Ready to Merge). Code target 8099336. Delivers the NNUE v2 architecture sweep as records/tooling (engine behaviour unchanged; net promotion is a scoped follow-up). AC#1: loss/NPS Pareto frontier over 14 one-factor candidates in artifacts/sweep-86.5/{sweep.json,RESULTS.md} — sweep.json is content-identical to the rig's, and the 4-net frontier (h128/h256/h512/h1024) independently re-derives as correct (h384 dominated by h512); swept factors + coverage limits recorded, conforming to the TASK-86.3 protocol (docs/nnue-architecture-sweep.md). AC#2: four frontier finalists SPRT vs gen-002 recorded in BENCHMARKS.md; independently corroborated against rig report.json/logs — h256 852-690-686/+25.9 PASS, h512 794-802-616/+28.0 PASS, h1024 130-197-309/-100.5 FAIL, h128 507-954-671/-26.8 FAIL, both sides identical binary sha256 with per-side EvalFile, commit 6793c34, fastchess 1.7.0, tc=10+0.1, elo0=0/elo1=5. AC#3: h256 selected (gen-002 arch on new corpus) with fixed-TC-Elo rationale (h256~h512 within error → width buys no Elo; corpus is the whole gain); report states LABEL-LIMITED. Verified: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, cargo test --workspace all clean on target; python -m unittest test_sweep → 26 pass; no .rs changed base..target so no hot-path bench needed; immutability confirmed (8099336 ancestor of tip a555fa3, post-target commit is handoff-only; engine/ and default.sbnn untouched).
<!-- SECTION:FINAL_SUMMARY:END -->
