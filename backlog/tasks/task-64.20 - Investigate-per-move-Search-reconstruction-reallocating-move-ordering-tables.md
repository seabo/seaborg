---
id: TASK-64.20
title: Investigate per-move Search reconstruction reallocating move-ordering tables
status: To Do
assignee: []
created_date: '2026-07-21 04:36'
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
