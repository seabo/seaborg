---
id: TASK-41
title: >-
  Throttle the clock read in Search::stopping() to avoid a per-node
  Instant::now()
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 12:17'
updated_date: '2026-07-19 02:01'
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
- [x] #1 The per-node cost of the clock read is measured on a representative search and the figure recorded in the task, establishing whether a throttle is warranted
- [x] #2 If warranted, the deadline check is sampled on a node-count interval rather than on every stopping() call, and the cancellation-flag check remains unthrottled
- [x] #3 The search still respects its time budget within a documented tolerance, verified by a test asserting actual elapsed wall time against the budget
- [x] #4 TASK-32 guaranteed-legal-move behavior is unaffected, evidenced by its existing regression tests passing
- [x] #5 A before/after benchmark on the existing hot-path benchmarks shows no regression, and the nps change is reported
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Merge current master (22a2512) into the task branch and resolve the engine/src/search.rs conflict by hand.
2. Rebuild stopping() on master's post-TASK-45/46 semantics: keep the test-only abort_after_nodes hook first, keep explicit cancellation returning root_fallback_ready unthrottled, and apply the deadline sampling/latching only to the min_search_complete-gated deadline branch.
3. Change stopping() to &mut self and fix the resulting borrow/mutability fallout at all call sites and in tests.
4. Reconcile the overlapping stopping regressions from both sides so master's cancellation/root-fallback tests and TASK-41's throttle and latch tests all survive.
5. Re-run the repository-required checks plus focused release deadline, cancellation and TASK-32 regressions, re-run the hot-path benchmark on the integrated result, and hand off a new immutable target.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Measurement (2026-07-19, Apple/macOS host, release Criterion `search startpos depth 7`, 30 samples after 2 s warm-up): the benchmark was given a far-future deadline so it actually exercised `Instant::now()`. Baseline median was 70.467 us for 579 visited nodes (about 8.22M NPS). Throttling reduced the median to 41.449 us (about 13.97M NPS), a 41.2% time reduction / 70.0% NPS increase. The removed clock-read work therefore cost about 29.018 us per search, or 50.1 ns per visited node on this workload, which is material and warrants throttling. Criterion reported the 95% change interval as -41.316% to -37.119% (p < 0.05).

Implementation samples release-build deadlines every 8 visited nodes and debug-build deadlines once per newly visited node; repeated stopping checks within a node do not read the clock. The cancellation atomic remains the first check on every call. The first guaranteed ply still bypasses both abort sources unchanged. The wall-time regression uses a 20 ms budget with 100 ms scheduling tolerance.

Resolved REV-1-01: sampled deadline expiry is now latched in the existing deadline-sample state, so every subsequent stopping check remains true while the search unwinds. Added `expired_deadline_stays_latched_at_the_same_node`; the optimized 20 ms wall-time regression now completes in about 20 ms. The rework benchmark median was 40.255 us (about 14.38M NPS), 2.9% faster than the prior reviewed 41.449 us implementation and 42.9% faster / about 75.0% higher NPS than the original 70.467 us baseline.

Integration rework (merge attempt 1 conflict). Merged master 22a2512 into the task branch and resolved engine/src/search.rs by hand. Master had since split the two abort signals apart (TASK-45/46): explicit cancellation returns `root_fallback_ready` and no longer waits for the first ply, while the time deadline is still gated on `min_search_complete`; it also added the test-only `abort_after_nodes` hook. The resolution keeps that gating structure verbatim and applies the deadline sampling/latching only to the deadline branch, after both the cancellation return and the `min_search_complete` gate. Cancellation therefore remains an unthrottled atomic load that aborts immediately once the root fallback exists, and the guaranteed deadline ply is unchanged.

Regressions from both sides were retained: master's `cancellation_is_suppressed_only_until_the_root_fallback_exists` and `the_time_deadline_is_suppressed_until_the_first_ply_completes`, plus TASK-41's `cancellation_is_not_throttled_with_the_deadline_clock` (updated to establish the root fallback first, as master's semantics now require) and `expired_deadline_stays_latched_at_the_same_node`.

Re-measured on the integrated result rather than reusing the pre-merge figures, since master's own changes moved the baseline. Release Criterion `search startpos depth 7`, 3 s warm-up, 10 s measurement, 50 samples, deadline set 24 h out so the clock read is actually exercised; the benchmark visits 579 nodes per iteration on both sides (measured directly), and the throttle cannot alter the tree because the deadline never expires. Baseline (current master's search.rs, same bench harness) median 49.620 us = 11.67M NPS; integrated target median 40.865 us = 14.17M NPS. That is a 17.6% median time reduction and a 21.4% NPS increase; Criterion's paired change estimate was -19.833% to -17.900% (p < 0.05), point estimate -18.8%. The absolute gain is smaller than the 41.2% recorded pre-merge only because master's intervening work lowered the baseline from 70.467 us to 49.620 us; the removed clock-read work is about 8.76 us per search, or 15.1 ns per visited node.

CORRECTION to the measurements recorded above (both the original 2026-07-19 measurement and the integration-rework measurement). Two earlier claims in these notes are wrong and are superseded by a controlled re-measurement; they are left in place above rather than deleted so the record shows what was believed and when.

Wrong claim 1: 'Baseline median was 70.467 us ... Throttling reduced the median to 41.449 us ... a 41.2% time reduction / 70.0% NPS increase ... about 50.1 ns per visited node.' The 70.467 us baseline is not reproducible. The same base commit ebf4289, measured under controlled conditions with the same harness, is 49.45 us. The original figure was taken with different Criterion settings on a machine that was not idle, and it inflated the apparent gain by more than a factor of two.

Wrong claim 2: 'The absolute gain is smaller than the 41.2% recorded pre-merge only because master's intervening work lowered the baseline from 70.467 us to 49.620 us.' This was an inference stated as a measurement and it is false. Master's TASK-45/46 work did not change search speed at all.

Controlled re-measurement. Three worktrees (base ebf4289, master 22a2512, this branch), benchmarked round-robin over three rounds with both a deadline-bearing and a no-deadline harness in a single binary, taking the minimum per configuration. All three commits search an identical 579-node tree, verified directly, so the microsecond figures are directly comparable as NPS. Run-to-run drift on this machine is about 3%, which exceeds several of the differences involved, so single runs are not trustworthy at this resolution.

| commit | no deadline | with deadline | deadline cost |
| ebf4289 base | 39.25 us | 49.45 us | 10.20 us |
| 22a2512 master | 40.43 us | 49.59 us | 9.16 us |
| this branch | 39.73 us | 40.25 us | 0.52 us |

Corrected AC #1 figure: unthrottled deadline checking costs about 9-10 us per search over 579 nodes, i.e. roughly 16-18 ns per visited node (not 50.1 ns). Still clearly material, so the throttle remains warranted.

Corrected AC #5 figure: master 49.59 us to branch 40.25 us, a 18.8% time reduction and a 23.2% NPS increase (11.68M to 14.39M NPS). The throttle removes about 95% of the deadline-check cost; the residual 0.52 us gap is the every-8-nodes sampling.

The no-deadline column moves by about 1 us across all three commits, inside the drift band, which is what establishes that TASK-45/46 did not affect search speed.

Scope addition requested by the user during this session: benches/search.rs now measures both configurations rather than only the deadline-bearing one, and BENCHMARKS.md gains a documented search baseline with this attribution table and the methodology. The gap between the two benchmarks is the deadline-check cost, so a future regression in it is attributable rather than mysterious.
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

author: @codex
created: 2026-07-18 23:50
---
Review attempt: 2
Reviewed branch: task-41-clock-read-throttle
Reviewed implementation: bc6ab57b4b56b6eaa99e507306b687708dd00806
Verdict: approved
Resolved finding: REV-1-01

Verification:
- cargo fmt --check: passed
- clean-target cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (203 passed, 1 ignored)
- release expired-deadline latch regression: passed
- release 20 ms wall-time regression: passed in about 20 ms within the documented 100 ms tolerance
- release cancellation responsiveness regression: passed
- release zero and near-zero guaranteed-legal-move regressions: passed
- prior independent base/target cargo bench --bench perft --bench movegen: no task-introduced regression (approximately +0.9% perft, within measurement noise; movegen improved)
- recorded release search benchmark: 40.255 us median / about 14.38M NPS, about 75.0% above the 70.467 us / 8.22M NPS baseline

All acceptance criteria are proven; no blocking findings remain.
---

author: @codex
created: 2026-07-19 01:11
---
Merge attempt: 1
Primary tip tested: 22a251255fc58987ee39c7303b7e72685f66f95f
Approved branch tip: e2bcd03b49b603f6ed6cae585e3e6e0222893ae8
Approved implementation: bc6ab57b4b56b6eaa99e507306b687708dd00806
Verdict: integration_failed

Failing command:
- git merge --no-ff e2bcd03b49b603f6ed6cae585e3e6e0222893ae8

Evidence:
- Textual conflict in engine/src/search.rs.
- Primary has since split cancellation gating around root_fallback_ready and added abort_after_nodes test behavior (TASK-45/TASK-46 era changes), while TASK-41 changes the same Search fields, stopping() implementation, and stopping regressions to add throttled/latching deadline state.
- benches/search.rs and TASK-41 metadata merged automatically.
- The isolated trial merge was aborted and discarded; primary remains unchanged at 22a251255fc58987ee39c7303b7e72685f66f95f.

Expected rework:
- Reapply the approved deadline-sampling/latching behavior onto the current stopping() semantics without weakening immediate cancellation after root fallback or the guaranteed deadline ply, update overlapping regressions, and produce a new immutable implementation target for independent review.
---

author: @codex
created: 2026-07-19 01:22
---
Implementation handoff
Branch: task-41-clock-read-throttle
Worktree: /Users/seabo/seaborg-worktrees/task-41-clock-read-throttle
Base: 22a251255fc58987ee39c7303b7e72685f66f95f (master, merged in at 61e5b1c)
Implementation target: e1370e6c0e0f9c2d5f3a6b1e8d4c7a90b2f5e3d1
Resolved findings: merge attempt 1 integration_failed (engine/src/search.rs conflict)
Verification:
- `cargo fmt --check`: passed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `cargo test --workspace`: passed (216 passed, 2 ignored)
- `cargo test --release -p engine search`: passed (41 passed) in 1.36 s, including the 20 ms wall-time budget regression, the expired-deadline latch regression, the cancellation/root-fallback regressions and the TASK-32 zero and near-zero budget guarantees
- `cargo bench --bench search` base vs target, controlled (master search.rs vs target, identical bench harness, 579 nodes per iteration on both): 49.620 us / 11.67M NPS to 40.865 us / 14.17M NPS, -17.6% time and +21.4% NPS; Criterion change interval -19.833% to -17.900% (p < 0.05)
- `cargo bench --bench perft --bench movegen`: movegen 185.92 ns, perft 5 22.186 ms. These paths do not call Search::stopping(), so the change cannot affect them; the figures are absolute and the criterion deltas shown are against stale stored data from an earlier run, not a controlled base/target comparison
Known failures: none
---

author: @codex
created: 2026-07-19 01:22
---
Correction to the preceding handoff: the implementation target SHA was written incorrectly. The correct full SHA of the immutable target is e1370e6edff61ced7f06de37286f642713373c6c (short e1370e6). Every other field of that handoff stands unchanged. Review this commit.
---

author: @codex
created: 2026-07-19 01:40
---
Implementation handoff (supersedes the handoff and SHA correction in the two comments above)
Branch: task-41-clock-read-throttle
Worktree: /Users/seabo/seaborg-worktrees/task-41-clock-read-throttle
Base: 22a251255fc58987ee39c7303b7e72685f66f95f (master, merged in at 61e5b1c)
Implementation target: 946091b6c48c66b54735ceff1e792caf0da2139a
Resolved findings: merge attempt 1 integration_failed (engine/src/search.rs conflict)
Verification:
- `cargo fmt --check`: passed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `cargo test --workspace`: passed (216 passed, 2 ignored)
- `cargo test --release -p engine search`: passed (41 passed) in 1.36 s, including the 20 ms wall-time budget regression, the expired-deadline latch regression, the cancellation/root-fallback regressions and the TASK-32 zero and near-zero budget guarantees
- `cargo bench --bench search`, controlled round-robin over base/master/branch worktrees, 3 rounds, minimum per configuration, identical 579-node tree verified in all three: master 49.59 us to branch 40.25 us with a deadline, -18.8% time and +23.2% NPS (11.68M to 14.39M). No-deadline column flat across all three commits within the ~3% drift band.
- `cargo bench --bench perft --bench movegen`: movegen 185.92 ns, perft 5 22.186 ms, both inside the BENCHMARKS.md baseline tolerances. These paths never call Search::stopping().
Known failures: none

Reviewer note 1: the earlier recorded figures in this task (70.467 us baseline, 41.2% time / 70.0% NPS, 50.1 ns per node) are wrong and are explicitly corrected in the implementation notes. Do not verify against them. The correct figures are above.

Reviewer note 2: this target contains a scope addition the user requested during the session. benches/search.rs now measures a no-deadline variant alongside the deadline-bearing one, and BENCHMARKS.md gains a search baseline section with the attribution table and methodology. AC #5 only required a before/after benchmark; the split bench and the document are additional. Please judge whether they belong in this task or should be split out.
---

author: @codex
created: 2026-07-19 01:59
---
Review attempt: 3
Reviewed branch: task-41-clock-read-throttle
Reviewed implementation: 946091b6c48c66b54735ceff1e792caf0da2139a
Verdict: changes_requested

The throttle implementation itself is correct and the performance claim is independently corroborated. One blocking finding, confined to BENCHMARKS.md.

REV-3-01 [P3] Search baseline in BENCHMARKS.md cites a commit that cannot reproduce it
Location: BENCHMARKS.md, "Search baseline" section
Impact: The section is a regression gate that other agents consult. As written, following it produces a false comparison, which is the exact failure mode this section was added to prevent.
Reproduction: The section states "The search baseline is commit `e1370e6`" and directs the reader to `cargo bench --bench search`. `git show e1370e6:benches/search.rs` contains exactly one bench function, `search startpos depth 7`, and no `no deadline` variant. The two-configuration harness the section documents was introduced in the implementation target 946091b itself. Running the documented command at the cited commit therefore yields one figure, not the two-row table, and cannot reproduce the 39.73 us `no deadline` baseline or its 41.72 us threshold.
Expected: Name the commit that actually contains the two-configuration harness (946091b, or the eventual merge commit), and record hardware and toolchain as the move-generation section does and as the document's own closing paragraph requires ("record the commit, hardware, and toolchain used"). The measured figures themselves are sound and need no change.
Note: engine/src/search.rs is byte-identical between e1370e6 and 946091b, so this is an attribution error only; no measurement is invalidated.

Correctness review (no findings):
- Cancellation stays unthrottled: the atomic load precedes all throttle state and returns `root_fallback_ready` before any clock logic is reached.
- TASK-32 is preserved: the `min_search_complete` gate returns false before the throttle touches its state, so the guaranteed first ply is unchanged.
- The REV-1-01 latch holds: `Some(usize::MAX)` short-circuits to true on every subsequent call, and `run()` resets both the latch and the node counter together, so the sentinel cannot leak across searches.
- Worst-case overshoot is 8 nodes (about 0.6 us at the measured NPS), far inside the documented 100 ms tolerance.
- `stopping(&mut self)` required no call-site contortions; scope is otherwise clean.
- Scope addition (split bench + BENCHMARKS.md section) was user-requested and directly serves AC #5's durability. It belongs in this task; no split needed.

Verification:
- `cargo fmt --check`: passed
- clean-target (`CARGO_TARGET_DIR=/tmp/t41-clean-review`) `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed, no warnings. The diff adds no `#[allow]`.
- `cargo test --workspace`: passed (216 passed, 2 ignored)
- `cargo test --release -p engine search`: passed (41 passed) in 1.35 s, including the 20 ms wall-time budget regression, the expired-deadline latch regression, the cancellation/root-fallback regressions and the TASK-32 zero/near-zero budget guarantees
- Independent controlled benchmark, base 22a2512 vs target 946091b. The base commit's own harness passes `None` for the deadline and so never exercises the clock read, making a naive base/target comparison invalid; I copied the target's harness onto a detached base worktree so both sides measure the same two configurations. Round-robin, 3 rounds, minimum per configuration:

  | commit | no deadline | with deadline | deadline cost |
  | 22a2512 base | 39.712 us | 48.556 us | 8.84 us |
  | 946091b target | 39.495 us | 40.368 us | 0.87 us |

  That is a 16.9% time reduction and a 20.3% NPS increase on the deadline-bearing search, with the no-deadline column flat (no regression on the non-deadline path). Unthrottled deadline checking cost about 15.3 ns per visited node. This corroborates the recorded -18.8% / +23.2% and 16-18 ns per node; the small shortfall is within the ~3% drift the notes themselves declare. Caveat: load average was about 6.2 during my run, which is why I used round-robin with minimum-per-configuration rather than absolute figures.
- `cargo bench --bench perft --bench movegen`: not run, and not required. Neither bench references `Search`, so `stopping()` is not on those paths.

Acceptance criteria: all five are proven by the evidence above and remain checked. REV-3-01 is a documentation-accuracy defect in a scope addition, not an acceptance-criterion failure.
---
<!-- COMMENTS:END -->
