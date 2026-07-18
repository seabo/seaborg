---
id: TASK-41
title: >-
  Throttle the clock read in Search::stopping() to avoid a per-node
  Instant::now()
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 12:17'
updated_date: '2026-07-18 23:46'
labels:
  - engine
  - search
  - performance
dependencies: []
priority: medium
type: bug
ordinal: 41000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search::stopping() (engine/src/search.rs:767-778) calls std::time::Instant::now() every time it is invoked, and it is invoked on the hot path: the iterative-deepening loop (search.rs:446, 454), the main search entry (search.rs:491), the interior move loop (search.rs:630), post-loop (search.rs:715), quiescence entry (search.rs:812), the quiescence move loop (search.rs:898) and the evasions loop (search.rs:931). That is roughly once per node and once per move, with no throttle.

Engines conventionally sample the clock only every N nodes (a mask test such as 'nodes & 4095 == 0') precisely because this read is not free. On macOS Instant::now() is a mach_absolute_time call; it is cheap relative to a syscall but not relative to a node, and it sits inside the innermost loops of the search.

The cost has not been measured in this repo, so the first job is to measure it rather than assume it. If it is material, throttle the clock read behind a node-count check while keeping the cancellation-flag read unthrottled (an atomic bool load is genuinely cheap, and TASK-39 is separately concerned with how responsive 'stop' is, so the flag should not become less responsive as a side effect).

Note the interaction with the TASK-32 guarantee: stopping() returns false outright until min_search_complete, so any throttle must not disturb that early return. Note also engine/src/trace.rs:141, which divides by elapsed microseconds and will divide by zero on a sufficiently fast search; that is adjacent and cheap to fix here if touched.

Identified during the TASK-38 investigation and deliberately left out of that ticket's scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The per-node cost of the clock read is measured on a representative search and the figure recorded in the task, establishing whether a throttle is warranted
- [ ] #2 If warranted, the deadline check is sampled on a node-count interval rather than on every stopping() call, and the cancellation-flag check remains unthrottled
- [ ] #3 The search still respects its time budget within a documented tolerance, verified by a test asserting actual elapsed wall time against the budget
- [ ] #4 TASK-32 guaranteed-legal-move behavior is unaffected, evidenced by its existing regression tests passing
- [ ] #5 A before/after benchmark on the existing hot-path benchmarks shows no regression, and the nps change is reported
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce REV-1-01 with the optimized focused deadline test and inspect the stopping/unwind call sequence.
2. Latch a sampled expired deadline for the remainder of the run while preserving the guaranteed-first-ply gate and per-call cancellation load.
3. Add a regression proving repeated stopping checks at the same node remain true after expiry, then run focused debug and release tests.
4. Re-run the hot-path benchmark and all repository-required checks, record REV-1-01 resolution evidence, and prepare a new immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Measurement (2026-07-19, Apple/macOS host, release Criterion `search startpos depth 7`, 30 samples after 2 s warm-up): the benchmark was given a far-future deadline so it actually exercised `Instant::now()`. Baseline median was 70.467 us for 579 visited nodes (about 8.22M NPS). Throttling reduced the median to 41.449 us (about 13.97M NPS), a 41.2% time reduction / 70.0% NPS increase. The removed clock-read work therefore cost about 29.018 us per search, or 50.1 ns per visited node on this workload, which is material and warrants throttling. Criterion reported the 95% change interval as -41.316% to -37.119% (p < 0.05).

Implementation samples release-build deadlines every 8 visited nodes and debug-build deadlines once per newly visited node; repeated stopping checks within a node do not read the clock. The cancellation atomic remains the first check on every call. The first guaranteed ply still bypasses both abort sources unchanged. The wall-time regression uses a 20 ms budget with 100 ms scheduling tolerance.

Resolved REV-1-01: sampled deadline expiry is now latched in the existing deadline-sample state, so every subsequent stopping check remains true while the search unwinds. Added `expired_deadline_stays_latched_at_the_same_node`; the optimized 20 ms wall-time regression now completes in about 20 ms. The rework benchmark median was 40.255 us (about 14.38M NPS), 2.9% faster than the prior reviewed 41.449 us implementation and 42.9% faster / about 75.0% higher NPS than the original 70.467 us baseline.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 23:32
---
Implementation handoff
Branch: task-41-clock-read-throttle
Worktree: /Users/seabo/seaborg-worktrees/task-41-clock-read-throttle
Base: ebf428924df7afef6616ad179b6c186d0faa4b6b
Implementation target: 9598721f1adcc43387ead42b544a510525579190
Resolved findings: none
Verification:
- `cargo fmt --check`: passed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `cargo test --workspace`: passed (203 passed, 1 ignored)
- `cargo bench --bench search -- --warm-up-time 2 --measurement-time 5 --sample-size 30`: median improved from 70.467 us to 41.449 us; derived NPS 8.22M to 13.97M (+70.0%)
Known failures: none
---

author: @codex
created: 2026-07-18 23:41
---
Review attempt: 1
Reviewed branch: task-41-clock-read-throttle
Reviewed implementation: 9598721f1adcc43387ead42b544a510525579190
Verdict: changes_requested

REV-1-01 [P1] Expired deadline is not latched across unwind checks
Location: engine/src/search.rs:842-853
Impact: Optimized searches do not reliably terminate at their deadline, so acceptance criterion #3 is not met and real time-controlled searches can continue far beyond their budget.
Reproduction: `cargo test --release -p engine search::tests::time_limited_search_honors_the_budget_after_the_guaranteed_ply -- --exact --nocapture` did not complete within 5 seconds for a 20 ms budget; the full release search-test run reported this test still running after 60 seconds.
Expected: Once a sampled deadline has expired, every subsequent stopping check must remain true while the search unwinds, without weakening the unthrottled cancellation check or the guaranteed-first-ply behavior.
Reasoning: The expired sample sets `last_deadline_check_nodes` and returns true once. The immediate next check at the same node satisfies the interval throttle and returns false, allowing search to resume instead of preserving the abort decision.

Verification:
- `cargo fmt --check`: passed
- clean-target `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `cargo test --workspace`: passed (203 passed, 1 ignored)
- debug timing/cancellation/TASK-32 regressions: passed
- optimized focused deadline test: failed to terminate within 5 seconds
- base/target `cargo bench --bench perft --bench movegen`: no task-introduced regression (movegen medians 196.06 ns base vs 194.03 ns target; perft medians 22.776 ms base vs 22.980 ms target, approximately +0.9% and within measurement noise)
---

author: @codex
created: 2026-07-18 23:42
---
Rework started for REV-1-01. The deadline-expired decision will be latched across unwind checks; cancellation responsiveness and the guaranteed-first-ply gate remain unchanged.
---

author: @codex
created: 2026-07-18 23:46
---
Implementation handoff
Branch: task-41-clock-read-throttle
Worktree: /Users/seabo/seaborg-worktrees/task-41-clock-read-throttle
Base: ebf428924df7afef6616ad179b6c186d0faa4b6b
Implementation target: bc6ab57b4b56b6eaa99e507306b687708dd00806
Resolved findings: REV-1-01
Verification:
- `cargo fmt --check`: passed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `cargo test --workspace`: passed (203 passed, 1 ignored)
- `cargo test --release -p engine search::tests::expired_deadline_stays_latched_at_the_same_node -- --exact`: passed
- `cargo test --release -p engine search::tests::time_limited_search_honors_the_budget_after_the_guaranteed_ply -- --exact --nocapture`: passed in about 20 ms
- `cargo bench --bench search -- --warm-up-time 3 --measurement-time 10 --sample-size 50`: 40.255 us median, about 14.38M NPS; 2.9% faster than prior reviewed implementation and about 75.0% higher NPS than original baseline
Known failures: none
---
<!-- COMMENTS:END -->
