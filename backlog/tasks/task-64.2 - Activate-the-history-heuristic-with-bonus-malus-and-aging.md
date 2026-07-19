---
id: TASK-64.2
title: 'Activate the history heuristic with bonus, malus and aging'
status: In Review
assignee:
  - '@george'
created_date: '2026-07-19 13:30'
updated_date: '2026-07-19 21:51'
labels:
  - search
  - move-ordering
dependencies: []
references:
  - engine/src/history.rs
  - engine/src/search.rs
  - engine/src/ordering.rs
parent_task_id: TASK-64
priority: high
type: bug
ordinal: 65000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The butterfly history table is allocated, reset per search, and read for quiet move ordering, but it is never written. Quiet moves are consequently ordered by an all-zero table, which is to say not ordered at all.

The only write site is commented out at search.rs:898-903, in the beta-cutoff branch where the killer move is stored. Both read sites are live: `MoveLoader::score_quiets` (search.rs:1488-1499) and `QMoveLoader::score_quiets` (search.rs:1548-1559) each call `history.get_unchecked` and assign the result as the move score. `history.reset()` is called at search.rs:551.

The effect is that the Quiet phase of the staged ordering yields moves in raw generation order. Since the Quiet phase sits between Killers and BadCaptures and covers the large majority of moves at most nodes, this is a substantial ordering loss on its own. It also blocks other work: reduction amounts for late move reductions, and the thresholds for move-count pruning, are conventionally driven by history scores, so those features cannot be tuned meaningfully while the table reads zero.

The table as it stands is a bare u32 butterfly table (history.rs:79-82) whose only mutation is `inc` (history.rs:98), an unguarded AddAssign. Reactivating the commented-out call alone would give a table that only ever grows, has no overflow guard, and never forgets. The work is therefore to make the heuristic correct rather than merely present: a depth-scaled bonus on the cutoff move, a malus applied to the quiet moves that were tried and failed before it, and a gravity or aging scheme that keeps values bounded and lets the table adapt within a search.

Whether history should be retained across moves within a game, rather than reset per search as it is today at search.rs:551, is an open question worth settling here and recording.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A quiet move causing a beta cutoff receives a depth-scaled history bonus
- [x] #2 Quiet moves searched and failing before the cutoff move receive a malus
- [x] #3 History values are bounded by a documented gravity or scaling scheme and cannot overflow their storage type
- [x] #4 Quiet moves in the ordering Quiet phase are demonstrably ordered by history score, verified by a test asserting a known good quiet is yielded before a known poor one after training the table
- [x] #5 The decision on whether history persists across moves within a game is recorded with rationale
- [x] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
- [x] #7 The history value read at the ordering sites is not narrowed by a truncating cast: search.rs:1499 and search.rs:1559 currently cast a u32 table value to i16, which wraps above 32767 and orders a repeatedly successful quiet move last
- [x] #8 A test drives a history value past the storage boundary of the ordering score type and asserts the move is still ordered ahead of an untrained move
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework driven by merge finding REV (codex-merge, comment #4): the approved history behavior, once integrated with TASK-64.14 tapered eval on master, makes gives_correct_answers pick a1b2 where it pinned a1b1 for KPvKP pawn-race FEN 8/6pk/8/8/8/8/P7/K7 w - - 0 1.

1. Merge pinned master tip cbec05f into the task branch to reproduce the combined state (done; failure reproduced).
2. Establish ground truth with the Syzygy KPvKP tablebase: a1b1 and a1b2 are both WIN (the only two winning moves); a2a4 DRAW, a2a3 LOSS. The suite pinned a single arbitrary optimal move, so it is over-specified and rejects an equally-optimal winning move that history/tapered-eval ordering legitimately surfaced.
3. Resolve by revalidating the expectation (sanctioned by the merge finding): change the suite best-move field from one &str to a slice of acceptable optimal moves and assert membership. The pawn-race entry accepts both a1b1 and a1b2 with a comment citing the tablebase result; every other entry keeps a single-element slice.
4. Preserve the approved history/search production behavior unchanged; no history.rs/search production changes in this rework.
5. Run cargo fmt --check, strict clippy, cargo test --workspace; record verification and hand off the integrated commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented signed butterfly history with depth-squared evidence and bounded gravity updates in [-32,768, 32,768]. A quiet beta cutoff receives the positive update; every fully searched quiet predecessor at that node receives the matching malus. History-to-ordering conversion saturates explicitly to i16, so a table value of 32,768 remains ahead of an untrained move instead of wrapping negative, while OrderedMoves retains its existing compact footprint.

Persistence decision: retain the existing per-search lifetime. Evidence is shared across iterative-deepening iterations within one Search::run, then reset when that run finishes; it does not persist across moves within a game. Search objects and their positions are request-specific today, so carrying this table across moves would require a new game-owned heuristic boundary and reset semantics. Keeping it local avoids leaking stale evidence across unrelated searches while still adapting throughout the tree where the gathered evidence is relevant.

TASK-27 strength smoke: baseline c7826f15b267cd89b0c1c02c97b5294f6ec9bf57 versus candidate working tree, optimized cargo build --release --bin seaborg, FastChess alpha 1.5.0, 4 paired-colour games at depth=4, concurrency=2, Hash=64, Threads=1. Result: non-authoritative INCONCLUSIVE, 2 wins / 0 draws / 2 losses, LLR 0.0 within [-2.94, 2.94], 0 forfeits, 0 crashes, runner exit 0. This smoke run establishes successful match integration but is too small to claim a strength result.

Rework (resolving merge finding, comment #4 @codex-merge).

Proximate failure: with the approved history heuristic integrated onto master (which had gained TASK-64.14 tapered evaluation), search::tests::gives_correct_answers picked a1b2 for the KPvKP pawn-race FEN 8/6pk/8/8/8/8/P7/K7 w - - 0 1 where the suite pinned a1b1. Reproduced on this branch after merging pinned master tip cbec05f.

Ground truth: probed the Syzygy KPvKP tablebase. From the root (White to move) a1b1 = WIN and a1b2 = WIN are the position's only two winning moves; a2a4 = DRAW and a2a3 = LOSS. The king must step aside before pushing, and the two king moves are equally optimal. The suite's single-move pin was therefore over-specified: it rejected an equally-optimal answer whenever quiet-move ordering surfaced the other king move. History ordering (now non-trivial) plus tapered eval legitimately flipped which of the two the depth-22 search returns first.

Resolution (test-only; no engine/production change): changed the correctness suite's best-move field from a single &str to a slice of acceptable optimal moves and assert membership; the pawn-race entry now accepts both a1b1 and a1b2 with a comment recording the tablebase result, and every other entry keeps its single optimum. history.rs is byte-identical to the approved target 88cbd082; the only search.rs delta since the master merge is inside the test module.

Persistence decision (AC#5) unchanged from the approved implementation: history retains per-search lifetime (reset per Search::run, shared across iterative-deepening iterations, not across game moves).

Integration strength smoke (TASK-27 script, on the combined code): baseline = master engine (code cbec05f, built at 18a4fa2) sha256 1db198cd...; candidate = integrated target 5d84701 sha256 23ed3d56...; FastChess alpha 1.5.0, smoke mode, 4 paired-colour games at depth=4, concurrency=2, Hash=64, Threads=1. Result: NON-AUTHORITATIVE INCONCLUSIVE, 2-0-2, LLR 0.0 within [-2.94, 2.94], 0 crashes, 0 forfeits, runner exit 0. Confirms clean match integration of the combined code; too small for a strength claim.

Master moved from cbec05f to 18a4fa2 during rework; the cbec05f..18a4fa2 delta is documentation and backlog files only (no Rust or Cargo changes), so it does not affect this integration. Base pinned at cbec05f per the recorded merge target.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-19 21:20
---
Implementation handoff
Branch: task-64.2-history-heuristic
Worktree: /Users/seabo/seaborg-worktrees/task-64.2-history-heuristic
Base: c7826f15b267cd89b0c1c02c97b5294f6ec9bf57
Implementation target: 88cbd086fa88c56cbd7d908588685397178d52c3
Resolved findings: none
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS
- cargo test --workspace: PASS (45 core, 274 engine passed / 2 ignored, 19 build metadata, 1 doc)
- python3 tools/strength/strength_test.py (smoke, 4 games, depth=4): INCONCLUSIVE, 2-0-2, LLR 0.0, no crashes or forfeits
Known failures: none
---

author: @george
created: 2026-07-19 21:21
---
Handoff correction: the full implementation target SHA is 88cbd082406a4442f1b78621a90116df94cc4145. The abbreviated 88cbd08 in git log was expanded incorrectly in comment #1; all other handoff fields and verification remain unchanged.
---

author: @codex-reviewer
created: 2026-07-19 21:31
---
Review attempt: 1
Reviewed branch: task-64.2-history-heuristic
Reviewed implementation: 88cbd082406a4442f1b78621a90116df94cc4145
Verdict: approved

All eight acceptance criteria are proven. The base-to-target diff is task-scoped; commits after the implementation target contain only handoff metadata.

Verification:
- cargo fmt --check: PASS
- CARGO_TARGET_DIR=/tmp/seaborg-review-task64-2-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS from a clean target
- cargo test --workspace: PASS (45 core; 274 engine passed, 2 ignored; 19 build metadata; 1 doc)
- cargo bench --bench perft --bench movegen: benchmark executables are byte-identical between base and target; no code-induced delta
- cargo bench --bench search -- 'search startpos depth 7': base 38.888/38.315 us versus target 39.162/38.707 us for deadline/no-deadline, approximately +0.7%/+1.0%, within the 5% gate
- TASK-27 strength smoke evidence: recorded in implementation notes, 2-0-2 with no crashes or forfeits
---

author: @codex-merge
created: 2026-07-19 21:35
---
Merge integration failure
Primary tip tested: cbec05f7527e820b688462930b5a889a368ceb2f
Detached merge tested: a6352c7b0c735c2b64eb7402a78127cdeb88de09
Overlapping landed task: TASK-64.14 in engine/src/search.rs

Failing command: cargo test --workspace
Evidence: search::tests::gives_correct_answers failed for FEN 8/6pk/8/8/8/8/P7/K7 w - - 0 1 at engine/src/search.rs:2852; integrated search selected a1b2 while the test expects a1b1. Result: 275 passed, 1 failed, 2 ignored. cargo fmt --check and clean-target strict Clippy passed before the test failure.

The detached trial was ejected and primary was not advanced. Rework must preserve the approved history behavior while resolving or deliberately revalidating this TASK-64.14 search expectation on the combined code.
---

author: @george
created: 2026-07-19 21:51
---
Implementation handoff (rework)
Branch: task-64.2-history-heuristic
Worktree: /Users/seabo/seaborg-worktrees/task-64.2-history-heuristic
Base: cbec05f7527e820b688462930b5a889a368ceb2f (pinned master tip merged at b71dcd55; current master 18a4fa2 differs only by docs/backlog files)
Implementation target: 5d84701744c905683508c0d129108a145f6a2643
Resolved findings: merge-integration finding (comment #4, @codex-merge) — gives_correct_answers a1b1/a1b2 expectation on the combined code
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (clean target CARGO_TARGET_DIR=/tmp/seaborg-task642-clippy)
- cargo test --workspace: PASS (core 45; engine 276 passed, 2 ignored; build metadata 19; doc 1)
- TASK-27 strength smoke on integrated code: NON-AUTHORITATIVE INCONCLUSIVE, 2-0-2, LLR 0.0 in [-2.94,2.94], 0 crashes, 0 forfeits, runner exit 0
Known failures: none
Note: rework is test-only. history.rs is byte-identical to approved target 88cbd082; the only search.rs change since the master merge is the correctness-suite revalidation, backed by the Syzygy KPvKP tablebase (a1b1 and a1b2 are the position's only two winning moves).
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Activated bounded signed butterfly history at implementation 88cbd082406a4442f1b78621a90116df94cc4145: quiet beta cutoffs receive depth-squared bonuses, failed predecessor quiets receive maluses, gravity bounds entries, and saturated ordering scores prevent i16 wraparound. Verified by focused regressions, cargo fmt, clean-target strict Clippy, workspace tests, recorded TASK-27 smoke evidence, and same-machine base/target benchmarks.
<!-- SECTION:FINAL_SUMMARY:END -->
