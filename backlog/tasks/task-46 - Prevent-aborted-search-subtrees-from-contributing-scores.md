---
id: TASK-46
title: Prevent aborted search subtrees from contributing scores
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 18:29'
updated_date: '2026-07-18 22:40'
labels: []
dependencies: []
references:
  - engine/src/search.rs
modified_files:
  - engine/src/search.rs
priority: high
type: bug
ordinal: 46000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When Search::stopping() is true, the search returns Score::zero() and unwinds. That zero is indistinguishable from a real draw score, so an aborted subtree can raise alpha, become best_move, or be written to the transposition table as a genuine evaluation. The engine then acts on a value that was never searched.

This is the same failure family as TASK-32 (illegal null move at fast time controls) and TASK-34 (self-play instability). Those were fixed at the reporting boundary; this is the underlying score-propagation path.

Audit every early return guarded by stopping() in the main search and quiescence search, and make aborted results unusable rather than plausible: the caller must be able to tell that a subtree was abandoned and must discard it instead of folding it into alpha, best_move, the PV, or the TT.

TODO site: engine/src/search.rs:815 (is this robust?).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An abort during a subtree search cannot cause that subtree value to raise alpha or be recorded as best_move
- [ ] #2 An abort during a subtree search cannot write an entry to the transposition table
- [ ] #3 An abort cannot corrupt the PV reported for the last completed iteration
- [ ] #4 A regression test drives a search to abort mid-subtree and asserts the returned bestmove matches the last fully completed iteration
- [ ] #5 The is this robust? TODO at engine/src/search.rs:815 is resolved and removed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Rework the deterministic abort regression for REV-1-01 so its threshold exercises the PV-corrupting pre-fix path and assert the complete preserved PV.
2. Prove the selected threshold aborts after entering a subtree, and verify the strengthened regression fails against the recorded base behavior while passing on the fixed target.
3. Run focused tests and all repository-required checks, commit the rework, record the finding resolution, and create a new immutable review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented explicit `Option<Score>` node outcomes across main search, razoring, quiescence, and check-evasion recursion. Aborted children unwind only after restoring the position and cannot update alpha, best move, PV, or ancestor TT entries. Iterative deepening now restores the prior completed PV when a candidate iteration aborts. Added a deterministic node-threshold regression that aborts within the depth-two subtree and verifies the depth-one result/PV/root TT entry remain authoritative.

Resolved REV-1-01: changed the deterministic abort threshold to the reviewer-proven pre-fix failure point and now compare the complete restored PV, not only its first move. The threshold fires on entry to the candidate depth-two search after depth one has fully completed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 21:55
---
Implementation handoff
Branch: task-46-aborted-search-subtrees
Worktree: /Users/seabo/seaborg-worktrees/task-46-aborted-search-subtrees
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Implementation target: 4905c1e5cfa5d5f585cec89b45c170c4c644bcbd
Resolved findings: none
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (198 passed, 1 ignored)
- cargo test -p engine search::tests: passed (25 passed)
Known failures: none
---

author: @codex
created: 2026-07-18 22:14
---
Review attempt: 1
Reviewed branch: task-46-aborted-search-subtrees
Reviewed implementation: 4905c1e5cfa5d5f585cec89b45c170c4c644bcbd
Verdict: changes_requested

REV-1-01 [P1] The AC#4 regression test passes without the fix
Location: engine/src/search.rs:1649 (mid_subtree_abort_keeps_the_last_completed_iteration), specifically the threshold at engine/src/search.rs:1661
Impact: AC#4 requires a regression test that drives a search to abort mid-subtree
and pins the last fully completed iteration. The committed test does drive a
mid-subtree abort, but it does not discriminate fixed from unfixed behavior: it
passes unchanged against the recorded base commit e301527, where aborted
subtrees still return Score::zero() and the aborted iteration's PV table is
never restored. A regression test that cannot fail on the pre-fix code provides
no protection against this bug recurring, so AC#4 is not provable as written.
The abort threshold `all_nodes_visited() + 5` lands in a region where base and
target behave identically.
Reproduction:
1. git worktree add --detach /tmp/task46-base e301527
2. Graft onto base only the test-only hook and the test body under review: the
   `#[cfg(test)] abort_after_nodes: Option<usize>` field, its `None`
   initializer, the `abort_after_nodes` early-return block in `stopping()`, and
   `mid_subtree_abort_keeps_the_last_completed_iteration` verbatim.
3. cargo test -p engine mid_subtree_abort_keeps_the_last_completed_iteration
   -> passes on the unfixed base commit.
Supporting evidence: an identical 300-point sweep of abort thresholds
(all_nodes_visited() + 1 ..= + 300) run on base and on the implementation
target differs at exactly two thresholds (+1 and +104), and only in the PV
field. `result` (score/best_move/depth) and the root TT entry depth are
identical at all 300 thresholds. The behavior this patch actually changes is PV
restoration, and +5 does not sample it.
Expected: The regression test must fail on the pre-fix code and pass on the
fixed code. Changing the threshold at engine/src/search.rs:1661 from
`all_nodes_visited() + 5` to `all_nodes_visited() + 1` achieves exactly that
and needs no other edit: verified FAILED on base (left: None, right:
Some(Move { orig: Square(8), dest: Square(16), ... }) at the
`search.pvt.pv().next()` assertion) and ok on target 4905c1e. Please also
confirm the chosen threshold remains a genuine mid-subtree abort rather than a
pre-first-move abort, and consider asserting the full PV line rather than only
its first move.

Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
  (re-run with a clean CARGO_TARGET_DIR to defeat lint caching; passed)
- cargo test --workspace: passed (203 passed, 1 ignored, 0 failed)
- grafted AC#4 test on base e301527: PASSED (should have failed) -> REV-1-01
- grafted AC#4 test on base e301527 with threshold +1: FAILED as required
- same test with threshold +1 on target 4905c1e: passed
- 300-threshold base-vs-target sweep: differs only at +1 and +104, PV field only
- cargo bench --bench perft --bench movegen (target): generate moves
  190.30 ns (BENCHMARKS.md investigate above 193.83 ns), perft 5 22.352 ms
  (investigate above 22.472 ms) -> both under threshold, and search.rs has no
  call path into either bench

Non-blocking observations (no action required for this verdict):
- The `search startpos depth 7` bench is repeatably ~1-6% slower on target than
  on base across three interleaved rounds (base 39.1/40.7/39.1 us, target
  41.6/41.3/41.4 us). That bench is degenerate (it reuses one warm Search and
  Table, so it measures TT-hit returns rather than real search work) and the
  machine was not perfectly idle, so this is reported for visibility rather than
  as a blocking regression. Worth re-confirming on an idle machine if search
  throughput matters.
- The regression test uses Table::new(1), which holds a single entry, so every
  node in the search collides on one slot. The `root_entry.depth == 1` assertion
  is therefore weaker than it looks; a larger table would make the "aborted node
  wrote no TT entry" claim for AC#2 more direct.

Reviewed the full base-to-target diff. The production change itself is sound:
NodeResult = Option<Score> is propagated correctly through search, razoring,
quiesce, and quiesce_evasions; every abort path unmakes the move before
unwinding (corroborated by the existing start_zob assertion in run()); aborted
nodes return before the Step 24 TT write; and iterative_deepening restores the
prior completed PV table. The TODO at the old engine/src/search.rs:815 is gone
(AC#5) and the diff adds no #[allow]. Only engine/src/search.rs and the task
file changed. The single blocker is the strength of the AC#4 test.
---

author: @codex
created: 2026-07-18 22:27
---
Review attempt: 1 (addendum - non-blocking evidence correction)
Reviewed implementation: 4905c1e5cfa5d5f585cec89b45c170c4c644bcbd
Verdict: unchanged (changes_requested, REV-1-01 still the only blocker)

Correcting the non-blocking benchmark observation in my previous comment. Those
numbers were taken while the machine was still recovering from a competing
engine test process, and they overstated the cost. I re-measured base e301527
against target 4905c1e on a settled machine (load 1.94 at start, benches
prebuilt so compilation would not perturb timing), four interleaved rounds,
--sample-size 20 --measurement-time 20, `search startpos depth 7`:

  round 1  base 39.069 us [38.909-39.213]  target 39.694 us [39.650-39.745]  +1.6%
  round 2  base 39.233 us [39.087-39.443]  target 39.784 us [39.698-39.916]  +1.4%
  round 3  base 39.105 us [39.033-39.221]  target 41.026 us [39.742-43.425]  +4.9% (wide CI, noisy sample)
  round 4  base 40.697 us [40.116-41.262]  target 39.802 us [39.744-39.884]  -2.2% (target faster)

The two tightest rounds agree on roughly +1.5%, and round 4 inverts the sign.
That is well under the 5% BENCHMARKS.md investigation threshold and consistent
with the cost of one predictable branch on the hot return path. My earlier
"repeatably 1-6% slower" reading was mostly measurement noise and should not be
treated as a performance concern.

This measurement carries forward to the next review round: the remedy for
REV-1-01 is a test-only change to the abort threshold at
engine/src/search.rs:1661, so the production hot-path code in the next target
will be byte-identical to what was benchmarked here and does not need
re-benchmarking unless production code changes.

The caveat about the bench itself still stands: `search startpos depth 7`
reuses a single warm Search and Table across iterations, so it measures
TT-hit returns rather than a cold search. It is a valid relative base-vs-target
comparison but not a measure of real search throughput.
---

author: @codex
created: 2026-07-18 22:40
---
Resolved REV-1-01: strengthened the abort regression at the discriminating +1 node threshold and asserted full-PV preservation. The review graft showed this threshold fails on base e301527 and it passes on the new target.
---

author: @codex
created: 2026-07-18 22:40
---
Implementation handoff
Branch: task-46-aborted-search-subtrees
Worktree: /Users/seabo/seaborg-worktrees/task-46-aborted-search-subtrees
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Implementation target: 08d38b98004072e70c0db7fdebf6f5d25d2d22b0
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (203 passed, 1 ignored)
- cargo test -p engine mid_subtree_abort_keeps_the_last_completed_iteration: passed
- reviewer graft of +1 threshold on base e301527: failed as required
Known failures: none
---
<!-- COMMENTS:END -->
