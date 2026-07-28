# NNUE architecture sweep — screen results (TASK-86.5)

Screen phase of the loss/NPS funnel from `docs/nnue-architecture-sweep.md`. This
records the produced Pareto frontier, the swept factors, and coverage limits.
**Final network selection is deferred to fixed-TC SPRT** (phase 2); loss/NPS only
decide who plays.

## Run configuration

- Engine: `target-cpu=native` release, commit `6793c34`. Rig: AMD Ryzen 9 3900XT, RTX 2070 SUPER.
- Corpus: `corpus-gen-002`, 103,086,342 records. Leak-free **by-shard** split (seed 0):
  92,764,818 train / 10,321,524 val; shards 000 + 018 held out.
- Fixed-everything-but-architecture: MSE win-prob loss, λ=0.3, 30 epochs, batch 8192,
  lr 1e-2, Adam, seed 0, scale 400, 8 decode workers.
- Quality axis: post-QAT quantized validation loss. Cost axis: realized single-thread
  in-engine NPS (`bench-positions.epd`, depth 13), the incremental-accumulator path.

## Swept factors (one at a time, 14 candidates)

- Feature-transformer width H: 128, 256, 384, 512, **1024** (width axis extended past the
  harness default of 512 to probe the capacity limit).
- Activation: CReLU vs SCReLU.
- Output buckets: 1, 4, 8, 16 (on a stack-16-32 v2 reference).
- Output-stack depth: (16), (16,32), (16,32,32).
- Dense-tail quantization: 0.5× and 2× on the reference stack.

### Coverage limits

- Width capped at H=1024 (largest single-layer transformer worth screening at this corpus size).
- Fixed 30-epoch budget for every candidate — this under-converges the bucketed nets (see below).
- One corpus, one machine, one build; NPS is comparable only within this run.

## Screen (all 14, by NPS)

| Candidate | Val loss | NPS |
| --- | ---: | ---: |
| width h128 | 0.011633 | 886,366 |
| **baseline h256** | 0.011288 | 750,458 |
| width h512 | 0.011119 | 676,860 |
| width h384 | 0.011183 | 674,175 |
| stack-depth h256 stack16 b8 | 0.011631 | 621,063 |
| tail-quant h256 stack16-32 b8 qb128-128-64 | 0.011715 | 544,367 |
| buckets h256 stack16-32 b16 | 0.012118 | 527,961 |
| buckets h256 stack16-32 b1 | 0.011429 | 524,798 |
| stack-depth h256 stack16-32-32 b8 | 0.012322 | 505,252 |
| buckets h256 stack16-32 b4 | 0.011833 | 495,865 |
| tail-quant h256 stack16-32 b8 qb32-32-64 | 0.012644 | 494,325 |
| buckets h256 stack16-32 b8 | 0.011944 | 487,852 |
| width h1024 | 0.011085 | 434,298 |
| activation h256 SCReLU | 0.011393 | 324,879 |

Reference: **gen-002** (`default.sbnn`, the shipped h256 net) measures **716,725 NPS** on the
same suite — as expected, next to the retrained h256 baseline (same architecture).

## Pareto frontier (4) and finalists

Frontier (non-dominated): **width h128, baseline h256, width h512, width h1024** — the entire
frontier is the plain-width CReLU axis.

Auto-selected finalists (even NPS sampling): **h1024, h256, h128**. Note this **skipped h512**,
the loss/NPS knee; h512 should be added to the phase-2 SPRT set (see below).

## Reading the frontier (feeds the AC#3 label-vs-capacity conclusion)

- **The width axis is the fair, clean comparison** (all widths share the mature feature-transformer
  + single-output CReLU path). Its frontier is well-behaved.
- **The v2 features (buckets / SCReLU / deeper stack / tail-quant) are all dominated — but the screen
  cannot fairly rank them.** Two own-engine biases: (1) the bucketed nets are *undertrained* under the
  fixed 30-epoch budget (per-bucket data splitting; b8 train loss 0.0116 > baseline 0.0110 — underfitting,
  not data-limited, confirmed by the train/val gap); (2) our inference *over-charges* the fresh-per-node
  output path (+0.57 µs/node for a tiny output stack, +1.75 µs/node for SCReLU — far above the added
  arithmetic). Do **not** read "buckets dominated" as "buckets bad."
- **Width is label-limited past ~h512.** The train/val gap climbs 1.2%→2.6%→3.9%→4.8%→**7.4%** across
  h128→h1024, with val loss flat at the top (h512→h1024: 0.011119→0.011085) while train keeps dropping.
  Beyond ~h512 the lever is *better labels* (datagen node budget + more positions), not more parameters.

## Status / next

- Phase-2 fixed-TC SPRT vs gen-002 is **pending go/no-go** (the multi-day compute).
- In progress: bucket higher-epoch (60) retrain to quantify the undertraining, and a gen-002-vs-new-corpus
  loss decomposition at fixed h256 (isolates the corpus contribution from the width contribution).
- The selected network + SPRT-grounded rationale (AC#2, AC#3) are recorded after phase 2.
