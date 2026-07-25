---
id: TASK-86.3
title: NNUE architecture-sweep methodology (decision doc)
status: In Review
assignee:
  - '@george'
created_date: '2026-07-25 12:23'
updated_date: '2026-07-25 18:35'
labels:
  - design
dependencies:
  - TASK-86.1
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
- [ ] #1 A decision document under docs/ specifies the by-game (not by-position) train/validation split protocol and the rationale (position correlation within a game)
- [ ] #2 It specifies the quality axis as post-QAT quantized validation loss with lambda, loss function, and training budget held fixed across candidates, and the cost axis as realized in-engine single-thread bench NPS via the incremental accumulator path
- [ ] #3 It specifies the screen (loss/NPS frontier) -> finalists -> fixed-time-control SPRT decision funnel, and states why static loss cannot be the final arbiter (eval quality changes the search tree)
- [ ] #4 It defines how to interpret frontier flattening as label-limited vs capacity-limited and what that implies for the next investment
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
<!-- COMMENTS:END -->
