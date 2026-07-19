---
id: TASK-61
title: Add benchmark-backed transposition-table hot-path enhancements
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 16:09'
labels:
  - transposition-table
  - performance
  - search
  - benchmark
dependencies:
  - TASK-60
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: medium
type: enhancement
ordinal: 60000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the identity policy, clean transposition-table rewrite, and search integration are stable, evaluate remaining hot-path opportunities rather than adopting them on folklore alone. The principal candidates are storing a position’s static evaluation to avoid duplicate work and support pruning, and prefetching child buckets before recursive search. Coordinate with TASK-50, TASK-51, and TASK-52 so metadata supports forthcoming pruning without coupling this task to those search changes. TASK-43 separately owns TT-assisted PV extension.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Representative fixed-depth positions and a reproducible benchmark establish baseline nodes, elapsed time, and probe behavior before hot-path changes
- [ ] #2 The value and validity conditions for a stored static evaluation are specified, including interaction with rule-sensitive evaluation from TASK-58; it is implemented only if measurements or imminent pruning consumers justify its entry-space cost
- [ ] #3 Child-bucket prefetching is evaluated on supported targets and retained only if it produces a repeatable benefit without harming portability or safety
- [ ] #4 Accepted enhancements include regression and benchmark coverage; rejected candidates have their measurements and decision recorded so the experiment is not repeatedly rediscovered
- [ ] #5 The final entry layout remains compact and its memory footprint and cache-line organization are asserted or tested
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a reproducible hash-loading search benchmark. The existing `search startpos depth 7` tree is 579 nodes and barely touches the table, so it cannot measure a TT hot-path change. Add a criterion group over representative fixed-depth positions whose trees are large enough to miss cache, and record baseline nodes, elapsed time and probe/hit/miss telemetry in BENCHMARKS.md.
2. Specify the value and validity conditions for a stored static evaluation before writing any code: what makes it reusable, how it interacts with the rule-sensitive evaluation policy, and what it costs in entry space given the data word has only 15 spare bits against the 16 an i16 eval needs.
3. Measure the static-eval candidate against the baseline (nodes, time, entry-space cost) and against the imminent pruning consumers in TASK-50/51/52. Implement only if the measurement or a concrete consumer justifies it; otherwise record the measurement and the decision.
4. Evaluate child-bucket prefetching: add a portable prefetch hint on the supported targets, issue it after make_move so the child cluster is in flight during the descent, and measure round-robin against the baseline. Retain only on a repeatable benefit with no portability or safety cost.
5. Add regression and benchmark coverage for whatever is accepted; write the measurements and rejection rationale for whatever is not into BENCHMARKS.md so the experiment is not rediscovered.
6. Assert the final entry layout: size, alignment, cluster-per-cache-line organisation and the reserved-bit invariant.
<!-- SECTION:PLAN:END -->
