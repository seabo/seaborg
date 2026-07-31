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

## gen-002 vs new-corpus loss decomposition (fixed h256, same val split)

Both evaluated on the new corpus's held-out val shards via `loss_decomp.py` (dequantize
the SBNN into an `NnueModel`, reuse the trainer's `_evaluate`; the retrained-h256
reconstruction reproduces the screen's 0.011288 exactly, validating the forward).

| Net | arch | trained on | val loss |
| --- | --- | --- | ---: |
| gen-002 (shipped) | h256 v1 | old bootstrap data | 0.012378 |
| retrained baseline | h256 v1 | new 103M corpus | 0.011288 |
| h512 (proposed gen3) | h512 v1 | new 103M corpus | 0.011119 |

- Corpus+recipe (same h256): −8.8% (0.012378 → 0.011288) — dominant.
- Width (h256 → h512): −1.5% (0.011288 → 0.011119).
- Total (gen-002 → gen3 h512): −10.2%.

The corpus upgrade is the big lever; width is secondary — consistent with the label-limited
reading. Loss is a proxy (gen-002's labels differ; some of the gap is distribution shift);
SPRT remains the strength arbiter.

## Phase 2 — fixed-TC SPRT and selection (AC#2, AC#3)

The four frontier finalists played SPRT vs gen-002 at `tc=10+0.1`, `elo0=0/elo1=5`,
α=β=0.05, on the rig (AMD Ryzen 9 3900XT, concurrency 11, fastchess alpha 1.7.0).
Recorded with full attribution in `BENCHMARKS.md`.

| Candidate vs gen-002 | Verdict | Elo | Games (W-D-L) |
| --- | --- | ---: | --- |
| h256 (gen-002 arch, new corpus) | PASS | +25.9 ± 10.7 | 2228 (852-690-686) |
| h512 (v2 width) | PASS | +28.0 ± 11.6 | 2212 (794-802-616) |
| h1024 | FAIL | −100.5 ± 22.2 | 636 (130-197-309) |
| h128 | FAIL | −26.8 ± 12.8 | 2132 (507-954-671) |

The games confirm the screen's reading exactly:

- **The corpus is the whole gain.** h256 — gen-002's own architecture retrained on the new
  corpus — already scores +25.9. h512 adds only +2.1 (inside the error bars): **width buys no
  measurable Elo.** The +28 gen3 result is ~93% corpus, ~7% width.
- **h1024 is −100 Elo** — the textbook "accurate-but-too-slow loses on the clock": its lower
  loss can't pay for a 434k-vs-717k NPS deficit at a real time control.
- **h128 is −27** — too little eval quality despite higher NPS.

**Selected network: h256** (gen-002 architecture, retrained on `corpus-gen-002`). h256 and
h512 are statistically indistinguishable and h256 is ~10% faster, so the width step is not
justified by fixed-TC Elo. h512 is a valid alternative if future richer data is expected to
reward the headroom.

**Label-limited vs capacity-limited (AC#3): label-limited.** At this size, more capacity does
not convert to Elo (h512 tied, h1024 −100), while the corpus upgrade alone gave +26. The
train/val gap widens with width (1.2%→7.4% across h128→h1024) with val loss flat past ~h512:
adding parameters fits the training labels but not held-out ones. The next investment is
**better labels** — higher datagen node budget / more positions / stronger self-play — not a
bigger network. Baking the selected net as the gen-003 default is a follow-up task.
