---
id: TASK-86.3
title: NNUE architecture-sweep methodology (decision doc)
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-25 12:23'
updated_date: '2026-07-25 21:41'
labels:
  - design
dependencies: []
parent_task_id: TASK-86
priority: high
ordinal: 145000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Before training many candidate architectures on the fixed corpus (TASK-81), pin the methodology as a decision record under docs/, so the sweep is fair and its results are trustworthy. Playing strength is a joint function of eval quality and search speed, so the objective is realized fixed-time-control Elo; the loss-vs-NPS frontier is a cheap screen that decides which few nets earn game matches. The doc must fix: the train/validation split protocol (held out by whole game/opening, not random position, to avoid leakage between correlated positions in one game); the quality axis (post-quantization-aware-training validation loss with lambda and loss fixed across runs); the cost axis (realized single-thread in-engine bench NPS using the incremental accumulator, not a standalone forward-pass microbenchmark); the screen -> Pareto-frontier -> fixed-TC SPRT funnel; and how to read the frontier's flattening as label-limited vs capacity-limited (a signal that the next lever is better labels, i.e. datagen node budget, rather than more parameters).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A decision document under docs/ specifies the by-game (not by-position) train/validation split protocol and the rationale (position correlation within a game)
- [x] #2 It specifies the quality axis as post-QAT quantized validation loss with lambda, loss function, and training budget held fixed across candidates, and the cost axis as realized in-engine single-thread bench NPS via the incremental accumulator path
- [x] #3 It specifies the screen (loss/NPS frontier) -> finalists -> fixed-time-control SPRT decision funnel, and states why static loss cannot be the final arbiter (eval quality changes the search tree)
- [x] #4 It defines how to interpret frontier flattening as label-limited vs capacity-limited and what that implies for the next investment
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Write docs/nnue-architecture-sweep.md as a decision record (modeled on docs/nnue-design-contract.md), fixing the sweep methodology.
2. AC#1: specify by-game (not by-position) train/val split; rationale = intra-game position correlation causes leakage. Note current trainer splits by random position (train.py:160) and the packed format carries no game id (selfplay/format.rs), so specify a concrete whole-game holdout mechanism (shard/contiguous-game-run granularity; format-bump option for finer grain).
3. AC#2: quality axis = post-QAT quantized validation loss with lambda, loss fn, training budget held fixed across candidates; cost axis = realized in-engine single-thread bench NPS via the incremental accumulator path (depends on TASK-86.1).
4. AC#3: screen (loss/NPS Pareto frontier) -> finalists -> fixed-TC SPRT funnel; static loss cannot be final arbiter because eval quality changes the search tree.
5. AC#4: reading frontier flattening as label-limited vs capacity-limited and the implied next investment (better labels / datagen node budget vs more parameters).
6. Run required checks (fmt/clippy/test — docs-only, unaffected) and hand off.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added docs/nnue-architecture-sweep.md, a decision record fixing the sweep methodology (modeled on docs/nnue-design-contract.md).

AC coverage:
- #1 by-game (not by-position) split + rationale (intra-game position correlation -> leakage). Notes the current trainer splits by random position (tools/trainer/train.py) and the packed format carries no game id (engine/src/selfplay/format.rs); specifies a concrete whole-game/shard holdout with a format-bump option for finer grain.
- #2 quality axis = post-QAT quantized validation loss with loss fn, lambda, and training budget held fixed; cost axis = realized in-engine single-thread bench NPS via the incremental accumulator path (not a standalone forward-pass microbenchmark).
- #3 screen (loss/NPS Pareto frontier) -> finalists -> fixed-TC SPRT funnel; three reasons static loss cannot be the final arbiter (eval changes the search tree; clock absent from loss; labels self-referential).
- #4 frontier flattening read as label-limited vs capacity-limited, with a concrete controlled-retrain test and the implied next investment (datagen node budget vs more/efficient parameters).

Docs-only change; no Rust touched. Dependency TASK-86.1 (incremental accumulator) is referenced as the cost-axis path but is not required to write the methodology.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-25 18:35
---
Implementation handoff
Branch: task-86.3-nnue-sweep-methodology
Worktree: /Users/seabo/seaborg-worktrees/task-86.3-nnue-sweep-methodology
Base: cfdac4d649599c9c5f117ada9b39dbc30110a875
Implementation target: 207fdb0ef4e2a2093cdb8208c4d7d5dec1f29bde
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (all suites ok)
Known failures: none
---

author: @george
created: 2026-07-25 21:41
---
Review attempt: 1
Reviewed branch: task-86.3-nnue-sweep-methodology
Reviewed implementation: 207fdb0ef4e2a2093cdb8208c4d7d5dec1f29bde
Verdict: approved

All four acceptance criteria proven by the delivered decision record (docs/nnue-architecture-sweep.md):
- AC#1 met: "The validation split must be by game, not by position" states the by-game/by-shard holdout decision and the intra-game position-correlation -> leakage rationale, including that it compresses the candidate gap the screen depends on.
- AC#2 met: "Quality axis" fixes post-QAT quantized validation loss with loss fn (MSE win-prob), lambda (0.3), training budget (epochs/lr/batch/optimizer/seed), and corpus held fixed; "Cost axis" fixes realized in-engine single-thread bench NPS via the incremental accumulator path, explicitly not a from-scratch forward-pass microbenchmark.
- AC#3 met: "The decision funnel" gives screen (loss/NPS Pareto frontier) -> finalists -> fixed-TC SPRT; "Why static loss cannot be the final arbiter" leads with eval-quality-changes-the-search-tree, plus clock-absent-from-loss and self-referential-labels.
- AC#4 met: "Reading the frontier" distinguishes label-limited vs capacity-limited and prescribes a single controlled retrain (fixed architecture on a higher-node-budget corpus) to tell them apart, with the implied next lever (better labels/datagen node budget vs more/efficient parameters).

Factual claims cross-checked against the repo and confirmed accurate:
- train.py: order = rng.permutation(len(data)); val_idx = order[:val_size] is a random by-position split (train.py:160).
- engine/src/selfplay/format.rs: 32-byte record (position, search score, outcome), no game id; FORMAT_VERSION reserved for a bump.
- nnue-design-contract.md: QB=64, lambda default 0.3, MSE in win-probability space, SBNN format, quantization-aware training default; CReLU/SCReLU and deferred HalfKA king buckets.
Referenced docs (nnue-design-contract.md, strength-testing.md) exist. No bare task-ID/AC/finding-ID citations in the doc.

Immutability: target 207fdb0 is an ancestor of branch tip b98fd2d; the only later commit is task-file handoff metadata; base(cfdac4d)..target diff is scoped to docs/nnue-architecture-sweep.md and the task file.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (no warnings)
- cargo test --workspace: pass (all suites; doc-tests ok)
- Docs-only change (no movegen/search hot path touched): speed benchmarks not applicable
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds docs/nnue-architecture-sweep.md, a decision record fixing the NNUE architecture-sweep methodology (companion to nnue-design-contract.md). It pins: the governing objective (realized fixed-TC Elo, not loss or NPS); a by-game/by-shard validation split with the intra-game-correlation rationale (AC#1); the quality axis as post-QAT quantized validation loss with loss fn, lambda, training budget, and corpus held fixed, and the cost axis as realized in-engine single-thread bench NPS via the incremental accumulator (AC#2); the screen->Pareto-frontier->fixed-TC-SPRT funnel with three reasons static loss cannot be the final arbiter, eval-changes-the-search-tree first (AC#3); and the label-limited vs capacity-limited reading of frontier flattening with a concrete controlled-retrain test and its implied next investment (AC#4). Docs-only; no Rust changed. Verified: all code/doc claims cross-checked against the repo (train.py:160 random by-position split; format.rs 32-byte record with no game id and a reserved version field; nnue-design-contract QB=64, lambda=0.3, MSE win-prob, SBNN, QAT default). cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings clean; cargo test --workspace pass.
<!-- SECTION:FINAL_SUMMARY:END -->
