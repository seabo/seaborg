---
id: TASK-46
title: Prevent aborted search subtrees from contributing scores
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-18 18:29'
updated_date: '2026-07-19 00:03'
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
- [x] #1 An abort during a subtree search cannot cause that subtree value to raise alpha or be recorded as best_move
- [x] #2 An abort during a subtree search cannot write an entry to the transposition table
- [x] #3 An abort cannot corrupt the PV reported for the last completed iteration
- [x] #4 A regression test drives a search to abort mid-subtree and asserts the returned bestmove matches the last fully completed iteration
- [x] #5 The is this robust? TODO at engine/src/search.rs:815 is resolved and removed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Merge the current primary tip into the persistent TASK-46 branch and resolve the four search recursion conflicts by combining TASK-46 abort propagation with TASK-54 child_bound() window semantics.
2. Run the focused TASK-46 abort/PV regressions and TASK-54 mate-parity regressions, inspecting test names and adding coverage only if the integrated behavior is not already exercised.
3. Run all repository-required checks, commit the integration rework, and record a new immutable implementation handoff for independent review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented explicit `Option<Score>` node outcomes across main search, razoring, quiescence, and check-evasion recursion. Aborted children unwind only after restoring the position and cannot update alpha, best move, PV, or ancestor TT entries. Iterative deepening now restores the prior completed PV when a candidate iteration aborts. Added a deterministic node-threshold regression that aborts within the depth-two subtree and verifies the depth-one result/PV/root TT entry remain authoritative.

Resolved REV-1-01: changed the deterministic abort threshold to the reviewer-proven pre-fix failure point and now compare the complete restored PV, not only its first move. The threshold fires on entry to the candidate depth-two search after depth one has fully completed.

Resolved REV-2-01: the iterative regression now aborts at completed-depth-one nodes + 2, which is the first recursive child of the depth-two root, and uses a 16 MiB TT. Added direct node-level coverage proving the aborted child returns None, restores the root move, contributes no PV move, and leaves the ancestor TT slot empty. On base e301527 the corresponding node API returns the plausible Score::zero() instead of an unusable outcome.

Resolved REV-3-01: sized the direct depth-two regression's PV table to depth two, so the assertion observes the row written by the searched root and fails on the unfixed base as demonstrated in review attempt 3.

Integrated primary commit f476d4c (including TASK-54) into the persistent task branch. Resolved all four search.rs conflicts by retaining TASK-46 Option<Score> abort unwinds and using TASK-54 child_bound() arguments at main-search, quiescence, and check-evasion recursion boundaries.
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

author: @codex
created: 2026-07-18 22:51
---
Review attempt: 2
Reviewed branch: task-46-aborted-search-subtrees
Reviewed implementation: 08d38b98004072e70c0db7fdebf6f5d25d2d22b0
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Verdict: changes_requested

REV-1-01 is resolved. I reproduced the discriminating claim independently:
grafting the new test (plus the `abort_after_nodes` hook) onto base e301527
fails there, so the test can now detect the pre-fix behavior.

REV-2-01 [P1] The strengthened test no longer aborts mid-subtree, leaving AC#1, AC#2 and AC#4 unproven
Location: engine/src/search.rs:1663 (`let abort_after = completed_iteration_nodes + 1;`)
Impact: The +5 -> +1 threshold change bought discrimination by moving the abort
out of the subtree entirely. AC#4 requires a test that "drives a search to abort
mid-subtree"; this test aborts on entry to the depth-two root, before any child
is searched, so no subtree is ever entered. Consequently the production
machinery this task exists to protect - the `let Some(child) = ... else {
self.pos.unmake_move(); return None }` unwind paths at engine/src/search.rs:706
and :721, the `?` propagation through `quiesce`/`quiesce_evasions`, and the
suppression of the Step 24 TT write for a node whose child aborted - is not
exercised by any regression test. AC#1 ("an abort during a subtree search cannot
raise alpha or be recorded as best_move") and AC#2 ("cannot write an entry to
the transposition table") therefore have no test evidence at all: the only
in-node abort coverage is `quiescence_abort_with_legal_evasions_is_not_checkmate`,
a unit test that enters `quiesce_evasions` already stopping. The test name
`mid_subtree_abort_keeps_the_last_completed_iteration` also misdescribes what it
does.
Reproduction:
1. Instrumented the target test to print the aborted iteration's node count:
   `completed_iteration_nodes=41 abort_after=42 final_total=42 depth2_nodes=1`.
   The depth-two iteration visits exactly one node - its own root - and returns
   `None` at Step 1 before making a move.
2. Grafted the test onto base e301527 with the assertions reordered so each
   reports separately. Only the PV assertion fails there:
   `panicked at engine/src/search.rs:1662: assertion left == right failed: AC3 pv assertion`.
   `assert_eq!(result, expected)` and both `root_entry` assertions pass unchanged
   on the unfixed base, so they discriminate nothing. The test currently proves
   AC#3 (iterative_deepening restores the prior completed PV table) and nothing
   more.
Expected: A regression test that aborts strictly inside the candidate
iteration's subtree - the abort must fire after the root has made a move and
recursed - and that still fails on base e301527. Note that a naive threshold
bump is not sufficient on its own: attempt 1 established that
`completed_iteration_nodes + 5` passes on base, so the chosen scenario must be
shown to fail there. If no single node threshold both enters a subtree and
diverges in the PV, the aborted subtree's effect on alpha/best_move and on the
TT likely needs to be asserted directly rather than inferred from the root PV.
A table larger than `Table::new(1)` would also make the AC#2 claim direct rather
than an artifact of every node colliding on one slot (carried forward from
attempt 1's non-blocking note).

Non-blocking observations (no action required for this verdict):
- The production change itself remains sound; I re-reviewed the full
  base-to-target diff and confirm attempt 1's assessment. `NodeResult =
  Option<Score>` propagates correctly through `search`, razoring, `quiesce` and
  `quiesce_evasions`; every abort path unmakes the move before unwinding;
  aborted nodes return before the Step 24 TT write; `iterative_deepening`
  restores `completed_pvt`. The `is this robust?` TODO is gone, satisfying AC#5.
- The rework delta from 4905c1e is confined entirely to `mod tests` (two hunks
  at engine/src/search.rs:1652 and :1667), so the production hot path is
  byte-identical to what attempt 1 benchmarked. Attempt 1's benchmark
  carry-forward holds and no re-benchmarking was needed.
- The diff adds no `#[allow]`. Only engine/src/search.rs and the task file
  changed.

Verification (on 08d38b9):
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed (also re-run with a clean CARGO_TARGET_DIR, no warnings)
- cargo test --workspace: passed (203 passed, 1 ignored)
- instrumented probe of the abort point on target: depth2_nodes=1 (abort at iteration root, not mid-subtree)
- grafted test on base e301527: fails only on the PV assertion; result and TT assertions pass on unfixed base
---

author: @codex
created: 2026-07-18 23:07
---
Resolved REV-2-01: the abort now fires after the depth-two root makes its first move and enters the child (exactly two candidate-iteration nodes). Direct coverage asserts None propagation, position restoration, no PV move, and no root TT write; iterative coverage asserts the returned bestmove and full PV remain from depth one.
---

author: @codex
created: 2026-07-18 23:07
---
Implementation handoff
Branch: task-46-aborted-search-subtrees
Worktree: /Users/seabo/seaborg-worktrees/task-46-aborted-search-subtrees
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Implementation target: 043d75f6bb010689887a37d916928dc941811471
Resolved findings: REV-1-01, REV-2-01
Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (203 passed, 1 ignored)
- cargo test -p engine mid_subtree_abort_keeps_the_last_completed_iteration: passed
- cargo test -p engine aborted_child_cannot_score_or_write_its_parent: passed
- base e301527 code-path comparison: aborted node returns Score::zero() rather than None, so the direct outcome assertion discriminates the fix
Known failures: none
---

author: @codex
created: 2026-07-18 23:27
---
Review attempt: 3
Reviewed branch: task-46-aborted-search-subtrees
Reviewed implementation: 043d75f6bb010689887a37d916928dc941811471
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Verdict: changes_requested

REV-2-01 is resolved on the "mid-subtree" half. I confirmed the +2 threshold is a
genuine in-subtree abort: `visit_node()` runs at the top of `search`, so node
n+1 is the depth-two root (which does not stop) and node n+2 is its first
recursive child, reached only after `make_move`. The new direct test
`aborted_child_cannot_score_or_write_its_parent` exercises the
`let Some(child) = ... else { self.pos.unmake_move(); return None }` unwind at
engine/src/search.rs:706, which attempt 2 correctly reported as untested.

REV-3-01 [P1] The PV assertion in the new direct test is vacuous, leaving AC#1 unproven
Location: engine/src/search.rs:1704-1707 (`assert!(search.pvt.pv().next().is_none())`),
root cause at engine/src/search.rs:1691 (`Search::new`)
Impact: This assertion is the only evidence offered for the best_move/PV half of
AC#1 ("an abort during a subtree search cannot cause that subtree value to raise
alpha or be recorded as best_move"). It cannot fail. `Search::new` builds
`PVTable::new(8)`, but the test searches at depth 2. `PVTable::pv()` reads
`data[0..self.depth]`, i.e. the row for `d == m == 8`, while `copy_to(2, mov)`
writes row `k = m - d = 6`. The assertion therefore inspects a row that nothing
in a depth-2 search ever writes, and passes regardless of whether the aborted
child's move was spliced into the PV. The remaining assertions in this test also
all pass on the unfixed base (see reproduction), so `assert_eq!(result, None)` -
which cannot be expressed on base at all, since `search` returns `Score` there -
is currently the test's only load-bearing claim.
Reproduction:
1. git worktree add --detach /tmp/task46-rev3-base e301527
2. Graft the `abort_after_nodes` hook (field, initializer, `stopping()` block)
   and this test onto base, dropping only the `assert_eq!(result, None)` line,
   which does not typecheck there.
3. cargo test -p engine <grafted test>
   -> PASSES on the unfixed base. Instrumented output:
   `nodes=2 zob_ok=true pv_first=None tt_empty=true`.
   The node-count, zobrist and TT assertions do not discriminate: the
   pre-existing `if self.stopping() { break 'move_loop; }` at
   engine/src/search.rs:681 already stops base after two nodes, and base's
   pre-existing Step 24 `stopping()` check already suppresses the root TT write.
4. Add one line to the grafted test: `search.pvt = PVTable::new(2);`
   -> now FAILS on base as required:
   `pv_first=Some(Move { orig: Square(8), dest: Square(16), ... })` (a2a3, the
   aborted child's root move spliced into the PV), panicking on the pv-none
   assertion.
5. The same one-line change on target 043d75f: passes (verified,
   `test result: ok. 1 passed`).
Expected: Size the test's PV table to the depth actually searched so `pv()`
reads the row the search writes - `search.pvt = PVTable::new(2);` after
`search.search_depth = 2;` is sufficient and needs no other edit. That converts
the assertion from vacuous to discriminating, and gives AC#1 and AC#2 the direct
evidence attempt 2 asked for.

Non-blocking observations (no action required beyond REV-3-01):
- `mid_subtree_abort_keeps_the_last_completed_iteration` at `+2` now passes on
  base e301527 (verified by graft), where the `+1` variant from attempt 2
  failed. This is not a second blocker: base leaves a depth-two-shaped `pvt`
  whose row-0 line reduces to the same single move as the completed depth-one
  PV, because the depth-two root searches the depth-one best move first. Once
  REV-3-01 is fixed, the direct test carries the discrimination burden - which
  is exactly the fallback attempt 2 proposed - and this test correctly serves
  AC#4's literal requirement (end-to-end bestmove/PV preservation) and AC#3. I
  do not want another threshold hunt; do not change this threshold.
- `Table::new(16)` resolves attempt 1's single-slot-collision note. The AC#2
  claim is now direct rather than an artifact of every node sharing one slot.
- The production change remains sound; I re-reviewed the full base-to-target
  diff independently. `NodeResult = Option<Score>` propagates correctly through
  `search`, razoring, `quiesce` and `quiesce_evasions`; every abort path unmakes
  the move before unwinding (`quiesce`/`quiesce_evasions` deliberately unmake
  before applying `?`); aborted nodes return before the Step 24 TT write;
  `iterative_deepening` restores `completed_pvt` and breaks. AC#5 is satisfied -
  the `is this robust?` TODO is gone. The diff adds no `#[allow]`. Only
  engine/src/search.rs and the task file changed.
- The delta from 4905c1e to 043d75f is confined to `mod tests` (two hunks), so
  production code is byte-identical to what attempt 1 benchmarked. Attempt 1's
  benchmark carry-forward holds and no re-benchmarking was performed or needed.

Verification (on 043d75f):
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
  (re-run with a clean CARGO_TARGET_DIR to defeat lint caching; no output)
- cargo test --workspace: passed (204 passed, 1 ignored, 0 failed)
- grafted direct test on base e301527: PASSED (should have failed) -> REV-3-01
- grafted direct test on base with PVTable::new(2): FAILED as required
- same one-line change on target 043d75f: passed
- grafted iterative test on base e301527: passed (see non-blocking note)
- instrumented node accounting: abort fires at node 2, inside the first child
---

author: @codex
created: 2026-07-18 23:32
---
Resolved REV-3-01: the direct abort regression now uses PVTable::new(2), making its no-principal-move assertion non-vacuous and discriminating against base e301527.
---

author: @codex
created: 2026-07-18 23:32
---
Implementation handoff
Branch: task-46-aborted-search-subtrees
Worktree: /Users/seabo/seaborg-worktrees/task-46-aborted-search-subtrees
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Implementation target: 0e4c7aca6bdc4715ddcc480f42ab21458ca7e691
Resolved findings: REV-1-01, REV-2-01, REV-3-01
Verification:
- cargo test -p engine aborted_child_cannot_score_or_write_its_parent: passed (1 passed)
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (204 passed, 1 ignored)
- review-attempt-3 graft on base e301527 with PVTable::new(2): failed as required
Known failures: none
---

author: @codex
created: 2026-07-18 23:38
---
Review attempt: 4
Reviewed branch: task-46-aborted-search-subtrees
Reviewed implementation: 0e4c7aca6bdc4715ddcc480f42ab21458ca7e691
Base: e30152795f22a10d8a50fc028dedf1dbb3567d90
Verdict: approved

REV-3-01 is resolved. The rework delta from 043d75f is exactly the one-line
remedy requested (`search.pvt = PVTable::new(2);` at engine/src/search.rs:1697)
and is confined to `mod tests`. I re-proved the discrimination independently
rather than relying on the handoff: grafting the `abort_after_nodes` hook and
the current `aborted_child_cannot_score_or_write_its_parent` body onto base
e301527 (dropping only `assert_eq!(result, None)`, which cannot typecheck
there) now FAILS on the unfixed base at the PV assertion:

  panicked at engine/src/search.rs:1292:
  PV ASSERTION: an aborted child must not become the principal move
  (got Some(Move { orig: Square(8), dest: Square(16), ... }))

That is a2a3, the aborted child's root move spliced into the principal
variation on base. The same test passes on target 0e4c7ac. The assertion is no
longer vacuous and now carries direct evidence for AC#1.

Acceptance criteria:
- AC#1 proven. The direct regression asserts the aborted child yields `None`,
  the root move is unmade (zobrist restored), and no principal move is
  recorded; it fails on base and passes on target. Structurally, both child
  call sites (engine/src/search.rs:706 and :721) unmake and `return None`
  before `value` can reach the alpha/best_move update.
- AC#2 proven. engine/src/search.rs:801 is the only production TT write;
  `quiesce` never writes. The `if self.stopping() { return None; }` at
  engine/src/search.rs:774 sits between the move loop and Step 24, and every
  abort propagation path (`?` in razoring/quiesce, the `else` unwinds, Step 1)
  returns earlier still. The test asserts the slot is empty using
  `Table::new(16)`, so the claim is direct rather than a single-slot artifact.
- AC#3 proven. `iterative_deepening` restores `completed_pvt` and breaks when a
  candidate iteration aborts; the iterative regression asserts the full
  restored PV equals the completed depth-one PV, and the direct regression
  demonstrates the corruption this prevents on base.
- AC#4 proven. `mid_subtree_abort_keeps_the_last_completed_iteration` aborts at
  `completed_iteration_nodes + 2` and asserts the returned `SearchResult`
  (score, best_move, depth) equals the depth-one result. I re-confirmed the
  threshold is a genuine mid-subtree abort: `visit_node()` is the first
  statement of `search`, so node n+1 is the depth-two root (which does not
  stop) and node n+2 is its first recursive child, reached only after
  `make_move`. The direct test's `assert_eq!(nodes, 2)` pins the same shape.
- AC#5 proven. No `is this robust` TODO remains anywhere under engine/src.

Verification (on 0e4c7ac):
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed,
  re-run with a clean CARGO_TARGET_DIR to defeat lint caching, no warnings
- cargo test --workspace: passed (204 passed, 1 ignored, 0 failed)
- cargo test -p engine aborted_child_cannot_score_or_write_its_parent: passed
- cargo test -p engine mid_subtree_abort_keeps_the_last_completed_iteration: passed
- grafted direct test on base e301527: FAILED at the PV assertion as required
- benchmarks: not re-run, and not required. I compared the pre-`#[cfg(test)]`
  region of engine/src/search.rs at 4905c1e and 0e4c7ac programmatically and
  they are byte-identical (6782 bytes each), so production code is exactly what
  attempt 1 benchmarked on a settled machine (roughly +1.5% on the degenerate
  `search startpos depth 7` bench, sign-inverting across rounds, well under the
  BENCHMARKS.md 5% threshold; perft and movegen unaffected and not on any call
  path from search.rs).

Scope and hygiene: only engine/src/search.rs and the task file changed between
base and target. The diff adds no `#[allow]`. The `abort_after_nodes` hook is
`#[cfg(test)]`-gated on the field, the initializer, and the `stopping()` branch,
so it cannot affect release behavior. The `run()` assertion
`assert_eq!(start_zob, self.pos.zobrist())` corroborates position restoration
across all abort paths under the full debug test suite. The new conservative
tail in `quiesce` (`if self.stopping() { None } else { Some(alpha) }`) can only
discard a value while the search is genuinely being abandoned, and
`min_search_complete` still guarantees the first ply cannot abort, so
`zero_time_limit_still_returns_a_legal_move` continues to hold.

Approved implementation SHA: 0e4c7aca6bdc4715ddcc480f42ab21458ca7e691
---

author: @codex
created: 2026-07-18 23:44
---
Merge attempt: 1
Primary tip tested: f476d4c8cfcbfd630235ad81067763b37af2e656
Approved target: 0e4c7aca6bdc4715ddcc480f42ab21458ca7e691
Result: ejected (textual conflict, primary not advanced)

Failing command:
  git merge --no-ff 0e4c7aca6bdc4715ddcc480f42ab21458ca7e691
  -> CONFLICT (content): Merge conflict in engine/src/search.rs (4 hunks)

Approval was intact and all preconditions passed: status was Ready to Merge,
`dependencies: []`, 0e4c7ac is an ancestor of branch tip 84967cc, and the only
file changed after the approved target is the task file. The eject is purely an
integration collision, not a defect in the approved work. The trial merge was
aborted and primary remains at f476d4c.

Cause: TASK-54 (cd66d14, `fix(task-54): preserve mate parity and detach UCI
input`) landed on primary after TASK-46's base e301527 and rewrote the exact
call sites TASK-46 rewrites. TASK-54 replaced raw score negation at recursion
boundaries with `Score::child_bound()` (engine/src/score.rs:64), which adjusts
mate distance by one ply instead of plain `-self`. TASK-46 rewrote the same
lines to propagate `Option<Score>`. Both edits are correct and they are
orthogonal in intent, but they overlap textually line for line.

Conflicting hunks, all in engine/src/search.rs:
1. Step 19 non-PV null-window child search
   primary: `self.search::<T, NonPv>(alpha.inc_one().child_bound(), alpha.child_bound(), depth - 1)`
   target:  `let child = self.search::<T, NonPv>(-alpha.inc_one(), -alpha, depth - 1);` + `else { self.pos.unmake_move(); return None; }`
2. Step 20 PV / re-search child search
   primary: `self.search::<T, Pv>(beta.child_bound(), alpha.child_bound(), depth - 1)`
   target:  same `Option` unwind shape with `-beta, -alpha`
3. `quiesce` move loop recursion
   primary: `self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound())`
   target:  `let child = self.quiesce::<T, Node>(-beta, -alpha);` then `score = child?.neg().inc_mate();`
4. `quiesce_evasions` move loop recursion — same shape as 3

Required resolution: keep BOTH changes at each site — primary's
`child_bound()` window arguments inside target's `Option<Score>` unwind. Do not
resolve by taking either side wholesale. Taking `ours` silently reintroduces the
aborted-subtree bug this task fixes; taking `theirs` silently reverts TASK-54's
mate-parity fix, which its own regression tests should catch.

Note also that `.neg()` on the target side and `child_bound()` on the primary
side are not interchangeable — `child_bound` adjusts mate distance where
`neg` does not — so the merged code must apply `child_bound()` to the window
arguments while leaving the existing `.neg().inc_mate()` treatment of the
returned child value as TASK-46 wrote it. Please re-run the full required
checks plus both TASK-46 regressions and TASK-54's mate-parity tests after
resolving, since this rework changes production code and voids the prior
approval.
---

author: @codex
created: 2026-07-18 23:49
---
Implementation handoff
Branch: task-46-aborted-search-subtrees
Worktree: /Users/seabo/seaborg-worktrees/task-46-aborted-search-subtrees
Base: f476d4c8cfcbfd630235ad81067763b37af2e656
Implementation target: 35b4994b5753d41d447e90de023b019775b5377b
Resolved findings: REV-1-01, REV-2-01, REV-3-01; merge-attempt-1 TASK-54 child_bound collision
Verification:
- cargo test -p engine aborted_child_cannot_score_or_write_its_parent: passed (1 passed)
- cargo test -p engine mid_subtree_abort_keeps_the_last_completed_iteration: passed (1 passed)
- cargo test -p engine child_mate_windows_preserve_distance_parity: passed (1 passed)
- cargo test -p engine child_bounds_invert_parent_mate_distance_conversion: passed (1 passed)
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (204 passed, 2 ignored)
Known failures: none
---

author: @codex
created: 2026-07-19 00:03
---
Review attempt: 5\nReviewed branch: task-46-aborted-search-subtrees\nReviewed implementation: 35b4994b5753d41d447e90de023b019775b5377b\nBase: f476d4c8cfcbfd630235ad81067763b37af2e656\nVerdict: approved\n\nThe merge-conflict rework correctly combines TASK-46 abort propagation with TASK-54 mate-aware child windows at all four overlapping recursion sites. Aborted children remain unusable Option outcomes, moves are restored before unwinding, the candidate PV is discarded, and no ancestor TT entry is written. The child search windows use child_bound() at main-search, quiescence, and evasion boundaries, preserving mate-distance parity.\n\nAcceptance evidence:\n- AC#1/#2: aborted_child_cannot_score_or_write_its_parent passes and directly asserts None, restored position, no PV move, and no root TT entry.\n- AC#3/#4: mid_subtree_abort_keeps_the_last_completed_iteration passes and preserves the completed depth-one result and full PV after aborting in the first depth-two child.\n- AC#5: the obsolete robustness TODO is absent.\n- Integration: both child_mate_windows_preserve_distance_parity and child_bounds_invert_parent_mate_distance_conversion pass.\n\nVerification:\n- cargo fmt --check: passed\n- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: passed\n- cargo test --workspace: passed (core 35, engine 169 passed/2 ignored, integration 5, doc test 1)\n- focused TASK-46 and TASK-54 regressions: all passed\n- git diff --check base..target: passed\n\nScope and immutability: base-to-target code changes are confined to engine/src/search.rs; the only commit after the implementation target changes the TASK-46 handoff file. No #[allow] was added. No new benchmark run was needed: relative to the previously approved TASK-46 production change, this rework only carries primary's already-reviewed child_bound() arguments into the same Option unwind structure; focused mate-parity regressions and the full suite cover the integration.\n\nApproved implementation SHA: 35b4994b5753d41d447e90de023b019775b5377b
---

author: @codex
created: 2026-07-19 00:03
---
Review attempt: 5 (formatting correction)
Reviewed implementation: 35b4994b5753d41d447e90de023b019775b5377b
Verdict: approved

Comment #15 contains escaped newline markers from CLI argument formatting. Its substance is unchanged. Verification passed: cargo fmt --check; uncached strict workspace Clippy; cargo test --workspace; focused TASK-46 abort/PV tests; and TASK-54 mate-parity tests. The base-to-target code diff is confined to engine/src/search.rs, and only task metadata follows the immutable implementation target.

Approved implementation SHA: 35b4994b5753d41d447e90de023b019775b5377b
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Integrated TASK-46's explicit aborted-node propagation with TASK-54's mate-aware child bounds. Verified cancellation cannot update alpha, best move, PV, or TT state using focused mid-subtree regressions; formatting, uncached strict Clippy, the full workspace suite, and mate-parity regressions all pass on 35b4994b5753d41d447e90de023b019775b5377b.
<!-- SECTION:FINAL_SUMMARY:END -->
