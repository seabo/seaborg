---
id: TASK-86.7
title: NNUE by-game validation split and architecture-sweep harness
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-27 17:46'
updated_date: '2026-07-27 20:30'
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
- [x] #1 The trainer supports a deterministic by-game / by-shard validation holdout that reserves whole datagen runs (shards) for validation via a fixed hash of run identity, derived from the corpus provenance manifest, and is reproducible byte-for-byte across invocations; the leaky by-position split is not used for sweep runs
- [x] #2 Unit tests prove no shard (hence no game) straddles the train/validation boundary and that the same corpus and seed yield the identical split on repeated runs
- [x] #3 A sweep-orchestration driver enumerates the methodology candidate architectures one factor at a time with all non-architectural configuration held fixed, and for each records post-QAT quantized validation loss and realized in-engine single-thread NPS with attribution (network parameter hash and binary commit)
- [x] #4 The driver computes the loss/NPS Pareto frontier, discarding every dominated candidate, and emits a machine-readable finalist selection plus the exact fixed-TC SPRT commands (tools/strength/strength_test.py) to run against the gen-002 default; unit tests cover the domination/frontier logic including ties and single-candidate cases
- [x] #5 Usage docs explain how to run the screen and the single-thread NPS protocol on the rig, consistent with docs/nnue-architecture-sweep.md and docs/strength-testing.md, including the fastchess prerequisite
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. split.py: deterministic by-shard (by-game) train/val split from corpus.manifest.json. Reconstruct each shard's contiguous record span in the concatenated corpus, hash each shard's run identity (opening_seed+file) with a fixed split seed, rank shards by hash, reserve lowest-hash whole shards for validation until cumulative records first reach val_fraction*total (never emptying train). Returns disjoint int64 train/val index arrays + the reserved shard set. Guards: manifest total_records must equal corpus len; degenerate (<2 shards, frac<=0/>=1, empty side) raises.
2. Wire train.py: add --split {by-position,by-shard} and --manifest PATH; when by-shard, build the split via split.py and pass precomputed (train_idx,val_idx) into train(); by-position stays the default legacy path. Refactor train() to accept an optional precomputed split.
3. test_split.py: no shard straddles the boundary (val indices are exactly whole-shard spans; train/val disjoint and cover the corpus), byte-identical split across repeated invocations for one corpus+seed, fraction is met, degenerate cases raise.
4. sweep.py: one-factor-at-a-time candidate enumeration from a fixed baseline architecture (hidden width, CReLU/SCReLU, output buckets, output-stack depth, dense-tail int8 scales) with all non-architectural config held fixed; dedup by canonical arch key. run_sweep orchestrator with injected train_and_export + measure_nps callables (datagen_campaign pattern) records post-QAT val loss and single-thread NPS with attribution (exported param hash + binary commit). Pareto frontier (drop weakly-dominated), finalist selection spanning the loss/NPS trade, and exact tools/strength/strength_test.py SPRT commands vs the gen-002 default; machine-readable JSON report. Production main() wires real train.py/export.py subprocess + a minimal single-thread UCI NPS reader over the committed bench suite.
5. test_sweep.py: domination/frontier incl. ties, single-candidate, all-dominated; finalist spanning; one-factor enumeration holds non-arch config fixed; SPRT command shape; end-to-end run_sweep with fake runners.
6. Docs: extend docs/nnue-architecture-sweep.md usage + tools/trainer/README.md with how to run the screen, the single-thread NPS protocol on the rig, and the fastchess prerequisite; keep consistent with strength-testing.md.
7. Run python unittest suites + repo-required cargo fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Delivered the two sweep-enabling tools as reviewable, unit-tested code; the multi-day GPU training and SPRT games remain TASK-86.5.

By-shard split (AC#1, AC#2): tools/trainer/split.py reconstructs each shard's contiguous record span from corpus.manifest.json and reserves whole lowest-hash shards for validation (fixed BLAKE2b hash of run identity folded with --split-seed) until they first cover --val-fraction, never emptying training. Wired into train.py via --split by-shard/--manifest/--split-seed; the trainer rejects a corpus whose record count disagrees with the manifest. test_split.py proves no shard straddles the boundary (val indices are exactly whole-shard spans; train/val disjoint and cover the corpus), byte-identical splits for one corpus+seed, the fraction is met minimally, degenerate cases (single shard, frac<=0/>=1) raise, and the train.py --split by-shard CLI path runs end-to-end on a tiny real corpus.

Sweep driver (AC#3, AC#4): tools/trainer/sweep.py enumerates candidates one factor at a time (width, CReLU/SCReLU, buckets, stack depth, dense-tail int8 quant) from a fixed baseline/v2 reference with all non-architectural config in TrainingConfig held fixed; dedups by canonical arch key. run_sweep records post-QAT validation loss and single-thread NPS with attribution (exported SBNN parameter hash + binary commit), computes the loss/NPS Pareto frontier (weak domination, strict in >=1), selects finalists spanning the trade, and writes sweep.json with the exact strength_test.py commands vs the gen-002 default. Heavy train/export/NPS steps are injected callables (datagen_campaign pattern); the CLI wires real train.py/export.py subprocesses and a single-thread UCI NPS reader over the committed bench suite. test_sweep.py covers domination/frontier incl. ties, single-candidate and all-dominated cases, finalist spanning, one-factor enumeration, the SPRT command shape, param-hash reading, and an end-to-end run with fake runners.

Docs (AC#5): tools/trainer/README.md gains a by-shard split note and an architecture-sweep section (how to run the screen, the single-thread NPS protocol on the rig, the fastchess prerequisite); docs/nnue-architecture-sweep.md gains a 'Running the screen' section tying the methodology to the tools. Consistent with docs/strength-testing.md.

Note: on-rig execution of the screen (real training/NPS/SPRT runs) is not verifiable in review by design — it is the TASK-86.5 campaign that depends on this. Reviewable here: the leak-free split and the frontier/finalist logic (Python unit tests).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-27 18:13
---
Implementation handoff
Branch: task-86.7-nnue-split-and-sweep
Worktree: /Users/seabo/seaborg-worktrees/task-86.7-nnue-split-and-sweep
Base: 0bedf79
Implementation target: 85333e4
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (all suites ok, incl. doc-tests)
- tools/trainer .venv/bin/python -m unittest discover -p 'test_*.py': pass (112 tests, incl. new test_split and test_sweep)
Known failures: none

Note: Python deps are not committed; the trainer README documents a .venv (numpy>=1.24, torch>=2.0). A local .venv was created only to run the suites (gitignored). torch 2.13 installed cleanly on Python 3.14 for CPU.
---

author: @george
created: 2026-07-27 20:30
---
Review attempt: 1
Reviewed branch: task-86.7-nnue-split-and-sweep
Reviewed implementation: 85333e4
Verdict: approved
Code target (immutable): 85333e4

All five acceptance criteria proven by objective evidence:
- AC#1/#2 (leak-free by-shard split): split.py reserves whole datagen runs via a fixed BLAKE2b hash of (split_seed, opening_seed, file); wired into train.py as --split by-shard with a manifest-vs-corpus record-count guard; by-position remains the legacy default. test_split.py proves no shard straddles the boundary (val indices are exactly whole-shard spans; train/val disjoint and cover the corpus) and byte-identical splits for one corpus+seed; degenerate cases raise; CLI path runs end-to-end. Manifest field names verified against tools/rl/datagen_campaign.py, and concat join order matches manifest order.
- AC#3 (driver): sweep.py enumerate_candidates varies one architectural factor at a time from fixed baseline/v2 reference with all TrainingConfig knobs held fixed; Screen records post-QAT val loss + single-thread NPS with attribution (SBNN param hash read from header + binary commit). test_sweep pins one-factor enumeration and end-to-end attribution.
- AC#4 (frontier + SPRT hand-off): pareto_frontier drops weakly-dominated points, select_finalists spans the trade, sprt_command emits shell-ready strength_test.py invocations vs the gen-002 default into sweep.json. Domination/frontier tests cover ties, single-candidate, all-dominated; SPRT flags cross-checked against the real tools/strength/strength_test.py interface.
- AC#5 (docs): tools/trainer/README.md and docs/nnue-architecture-sweep.md document running the screen, the single-thread NPS protocol on the rig, and the fastchess prerequisite; the #installing-fastchess anchor and referenced paths all resolve.

On-rig execution (real training/NPS/SPRT) is out of scope by design (TASK-86.5 depends on this); the reviewable core is fully verified here.

Verification (on target 85333e4):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (exit 0)
- cargo test --workspace: pass (exit 0, pipefail; no FAILED)
- tools/trainer .venv/bin/python -m unittest discover -p 'test_*.py': 112 tests OK

No engine hot-path Rust changes (diff is Python tooling + docs), so no perft/movegen benchmark required. No new #[allow] introduced. Diff scope is clean (split.py, sweep.py, tests, train.py, docs, task file). Comments are self-contained and reason-stating.

Verdict: Ready to Merge. Code target: 85333e4.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Delivered the two sweep-enabling tools (leak-free by-shard validation split and the architecture-sweep screen) as reviewable, unit-tested in-repo Python; on-rig training/NPS/SPRT execution is out of scope (TASK-86.5). split.py reconstructs each shard's contiguous record span from corpus.manifest.json and reserves whole lowest-hash runs for validation via a fixed BLAKE2b hash of run identity folded with --split-seed, wired into train.py as --split by-shard (by-position stays the legacy default) with a manifest-vs-corpus record-count guard. sweep.py enumerates candidates one factor at a time with all non-architectural config held fixed, records post-QAT val loss and single-thread NPS with attribution (param hash + binary commit), computes the loss/NPS Pareto frontier, selects finalists spanning the trade, and emits sweep.json plus exact strength_test.py SPRT commands vs the gen-002 default. Verified on target 85333e4: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -D warnings clean (exit 0); cargo test --workspace pass (exit 0, pipefail); tools/trainer unittest suite 112 tests OK (incl. test_split leak-freedom/determinism and test_sweep domination/frontier/finalist/SPRT). SPRT command flags cross-checked against tools/strength/strength_test.py; manifest fields (file/opening_seed/records/total_records) cross-checked against tools/rl/datagen_campaign.py; doc cross-references (fastchess anchor, rig README, bench-positions.epd, default.sbnn) all resolve.
<!-- SECTION:FINAL_SUMMARY:END -->
