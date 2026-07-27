---
id: TASK-86.7
title: NNUE by-game validation split and architecture-sweep harness
status: To Do
assignee: []
created_date: '2026-07-27 17:46'
labels:
  - nnue
  - tooling
dependencies:
  - TASK-86.3
  - TASK-86.4
  - TASK-81
documentation:
  - docs/nnue-architecture-sweep.md
  - docs/strength-testing.md
parent_task_id: TASK-86
priority: high
type: task
ordinal: 157000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build the in-repo tooling the architecture sweep (TASK-86.5) needs before it can run fairly, so that the campaign itself is reduced to training the enumerated candidates and playing the SPRT matches.

Two gaps block a trustworthy sweep today:

1. The methodology (docs/nnue-architecture-sweep.md) mandates a by-game / by-shard validation split, but tools/trainer/train.py still takes a random by-position split (rng.permutation over all positions). Positions within one self-play game are near-duplicates sharing the same outcome label, so a by-position split leaks near-twins across the train/val boundary: it makes validation loss optimistic and, worse for a sweep, compresses the gap between candidates the screen depends on. The packed record (engine/src/selfplay/format.rs) carries no game id, but the corpus is concatenated from many independent datagen runs and concat_samples.py emits a provenance manifest (corpus.manifest.json) recording shard boundaries; a whole run is a superset of whole games, so reserving entire runs for validation via a deterministic hash of run identity yields a leak-free split with no format change.

2. There is no sweep-orchestration driver. Running the screen means enumerating the candidate architectures (feature-transformer width H, CReLU vs SCReLU, output buckets, output-stack depth, dense-tail quantization) one factor at a time with every non-architectural knob held fixed (loss fn, lambda, epochs/lr/batch/optimizer/seed, corpus, split), training and exporting each as a QAT-quantized SBNN, recording post-QAT quantized validation loss and realized in-engine single-thread NPS with attribution, and computing the loss/NPS Pareto frontier to pick the finalists that earn game matches.

This task delivers both as reviewable, unit-tested in-repo code and stops at producing the finalist selection plus the exact SPRT commands to run. It does NOT run the multi-day GPU training or the thousands of fixed-TC SPRT games; that supervised rig campaign is TASK-86.5, which depends on this task. Landing this first gives review agents something they can actually verify (the leak-free split and the frontier logic), and leaves 86.5 as the pure run-and-select campaign.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The trainer supports a deterministic by-game / by-shard validation holdout that reserves whole datagen runs (shards) for validation via a fixed hash of run identity, derived from the corpus provenance manifest, and is reproducible byte-for-byte across invocations; the leaky by-position split is not used for sweep runs
- [ ] #2 Unit tests prove no shard (hence no game) straddles the train/validation boundary and that the same corpus and seed yield the identical split on repeated runs
- [ ] #3 A sweep-orchestration driver enumerates the methodology candidate architectures one factor at a time with all non-architectural configuration held fixed, and for each records post-QAT quantized validation loss and realized in-engine single-thread NPS with attribution (network parameter hash and binary commit)
- [ ] #4 The driver computes the loss/NPS Pareto frontier, discarding every dominated candidate, and emits a machine-readable finalist selection plus the exact fixed-TC SPRT commands (tools/strength/strength_test.py) to run against the gen-002 default; unit tests cover the domination/frontier logic including ties and single-candidate cases
- [ ] #5 Usage docs explain how to run the screen and the single-thread NPS protocol on the rig, consistent with docs/nnue-architecture-sweep.md and docs/strength-testing.md, including the fastchess prerequisite
<!-- AC:END -->
