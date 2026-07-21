---
id: TASK-64.20
title: Investigate per-move Search reconstruction reallocating move-ordering tables
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-21 04:36'
updated_date: '2026-07-21 16:34'
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
