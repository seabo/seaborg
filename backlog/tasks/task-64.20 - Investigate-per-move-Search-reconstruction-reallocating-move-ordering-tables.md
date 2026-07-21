---
id: TASK-64.20
title: Investigate per-move Search reconstruction reallocating move-ordering tables
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-21 04:36'
updated_date: '2026-07-21 16:42'
labels:
  - search
  - move-ordering
  - performance
  - architecture
dependencies: []
parent_task_id: TASK-64
priority: medium
ordinal: 126000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SearchEngine::start_inner (engine/src/search.rs) spawns a worker thread that constructs a fresh Search per `go` command -- i.e. once per move played in a game -- via Search::with_events, and drops it when the search ends. Every per-Search move-ordering table is therefore reallocated and zeroed on every move: HistoryTable (~32KB), KillerTable (~2KB), and, after TASK-64.10, ContinuationHistory (~4.72MB) and the counter table (~3KB).

This is a suspected per-move overhead, not a confirmed bottleneck. Evidence for: a multi-megabyte allocation now recurs every move rather than being amortised across a game. Evidence against: the TASK-64.10 fixed-depth ablation measured per-search throughput (nps) comparable to the feature-off baseline, and `vec![0; N]` is lazily zeroed so a shallow search faults in only the pages it touches. The one alarming signal -- a strength-test preflight bestmove of 0.869s for the TASK-64.10 candidate vs 0.029s for baseline -- is a single noisy sample and may be process-start noise. The question is directly relevant to TASK-64.10, whose fast-TC strength SPRT came back INCONCLUSIVE with a mildly negative point estimate; per-move construction cost is one candidate explanation that has not been ruled out.

The conventional fix, if the cost is material, is to make the large per-worker ordering tables persist across searches within a worker -- allocated once and cleared cheaply between searches (a memset/fill rather than a fresh allocation) -- instead of rebuilding them per move. This interacts with the Lazy SMP boundary (TASK-64.16): whatever lands must be per-worker.

Distinct from TASK-64.19, which reuses the per-node OrderedMoves buffer inside a single search; this task is about the per-move reconstruction of the Search struct and its history/killer/continuation/counter tables. Do the measurement first and let it decide whether the fix is warranted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The per-move cost of reconstructing Search and its ordering tables (history, killers, continuation history, counter) is measured in isolation from search work, and the figures are recorded
- [ ] #2 The measurement result is used to decide whether a fix is warranted; if the cost is immaterial the task records that and closes without a code change
- [ ] #3 If warranted, the large per-worker ordering tables persist across searches within a worker (allocated once, cleared between searches) rather than reallocated per move, with the per-worker/Lazy-SMP ownership arrangement documented
- [ ] #4 Fixed-depth node counts are identical before and after any change, confirming cheap clearing is behaviourally equivalent to fresh allocation
- [ ] #5 A before/after measurement (per-move construction cost, and a fast-TC throughput or strength sanity check) is recorded showing the overhead removed or demonstrated immaterial
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Baseline = b3fd63c (master + task-64.8 merge, per user). Tables present: HistoryTable 32KB inline, KillerTable ~2KB, CounterMoveTable ~3KB boxed, ContinuationHistory 4.72MB boxed. All built fresh per Search::build (once per 'go'/move).
2. Measure the per-move Search reconstruction cost in ISOLATION from search work (AC#1): time Search::new construction (fresh alloc of all four tables + EvalState + stack Box) and the four-table reset()/fill path, in release mode, on representative positions. Compare against a realistic per-move search budget at fast TC.
3. Note current architecture already calls history/kt/counter/cont_hist reset() at the END of run() (search.rs:1116-1119) immediately before drop(search) in start_inner -- redundant full memset in the per-move-construction model. Quantify it.
4. Decide (AC#2): if construction cost is immaterial vs per-move search time, record figures and close with NO code change. If material, escalate scope decision (user directed: investigation, no code).
5. Record before/after / attribution figures in task notes (and BENCHMARKS.md if a durable figure). Confirm fixed-depth node counts are unaffected by construction path (AC#4 is trivially satisfied if no code change).
6. Handoff for independent review; do not self-approve.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
INVESTIGATION FINDINGS (no code change; branch carries only lifecycle commits)

Baseline: b3fd63c (master + task-64.8 merge, per user direction). Hardware: Apple M3 Pro. All timings release-mode.

== Measurement method (AC#1) ==
A temporary Criterion bench (benches/search_construction.rs, reverted after measurement so the branch stays clean) timed, in isolation from search work:
- Real per-move construction via the production path Search::new (same build() as with_events): fresh alloc of all four ordering tables + EvalState + ply-stack box.
- The dominant term isolated: fresh 4.72MB ContinuationHistory alloc (vec![0i32; 2*768*768] -- the exact production call).
- The cheap-clear alternative (AC#3 candidate): fill(0) memset over an already-allocated 4.72MB grid.
Per-move search wall time / nps taken from the real UCI binary (go movetime), keeping stdin open so the async search runs to its deadline.

== Figures (Criterion medians) ==
Search::new construction: startpos 238us, kiwipete 176us, middlegame 193us (~190us typical).
Isolated 4.72MB ContinuationHistory fresh alloc: ~169us -- i.e. ~89% of total construction cost is this one allocation.
Same grid cleared via fill(0): ~23us (~7x cheaper than fresh alloc).
Representative per-move search (middlegame, go movetime 100): depth 8 at time=47ms nps=625k; depth 9 nodes=51062 time=58-70ms nps=0.72-0.87M.

== Table sizes ==
ContinuationHistory 4.72MB boxed (dominant); HistoryTable ~32KB inline; CounterMoveTable ~3KB boxed; KillerTable ~2KB. Only ContinuationHistory is large enough to matter.

== Decision (AC#2): immaterial -- no code change ==
Per-move construction (~190us) against a realistic fast-TC per-move budget of tens of ms is ~0.2-0.6% (190us/30ms=0.63%, /47ms=0.40%, /100ms=0.19%). Even at an extreme 5ms/move it is under 4%. This is far below the noise floor of a strength SPRT and cannot be the dominant cause of the TASK-64.10 INCONCLUSIVE fast-TC result (whose CI spans tens of Elo).

The alarming 0.869s-vs-0.029s preflight bestmove is NOT explained by per-move construction: 0.869s is ~4600x the ~190us construction cost, it is a one-time process-start / cold-cache / page-in effect (binary load, 16MB TT alloc, first-search faults), and it was a single sample. Construction cost is identical on every move, so it cannot produce a one-move outlier.

== The would-be fix, quantified ==
Persisting the per-worker tables and clearing with fill(0) between searches would save ~146us/move (169us alloc -> 23us memset). Additionally, run() already calls history/kt/counter/cont_hist reset() at search.rs:1116-1119 immediately before drop(search) in start_inner -- a redundant full memset on memory about to be freed in the current per-move-construction model (~23us+). Total avoidable overhead ~170us/move (~0.4% at 47ms/move). Not worth the added complexity plus the per-worker ownership surface it would introduce at the Lazy SMP boundary (TASK-64.16). Recorded as an observation for a human scope decision; no follow-up created by this agent.

== Behavioural equivalence (AC#4) ==
No code change, so before==after by construction. Additionally verified determinism: fixed go depth 9 on the middlegame gives nodes=51062 identically across two separate freshly-constructed searches. A fresh calloc and a fill(0) both yield all-zero tables, so the allocation strategy cannot alter search decisions or node counts.

== Sanity check (AC#5) ==
No code change => before/after per-move construction cost and throughput are identical. The ratio above (construction is 0.2-0.6% of per-move search time at fast TC) is the recorded evidence that the overhead is immaterial.

Verification: cargo fmt --check OK; cargo clippy --workspace --all-targets --all-features -D warnings OK; cargo test --workspace OK (all suites green).
<!-- SECTION:NOTES:END -->
