---
id: TASK-88
title: >-
  Investigation: where Seaborg's search loses effective depth (selectivity
  profile)
status: To Do
assignee: []
created_date: '2026-07-27 10:15'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
  - engine/src/pv_table.rs
  - BENCHMARKS.md
priority: high
type: spike
ordinal: 150000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-82 established that a large part of Seaborg's strength gap is selectivity: at equal time we reach far fewer plies and our effective branching factor is higher. The two techniques we reached for first - SEE main-search pruning (TASK-64.21) and singular extensions (TASK-64.13) - both measured marginal, so the win is not in those obvious individual prunings. This investigation locates where OUR search actually loses effective depth by instrumenting and measuring Seaborg itself, and produces a ranked set of first-principles experiments to try.

Philosophy (see AGENTS.md). This is NOT a gap-closing exercise against Stockfish. Do not diff our behaviour against another engine situation-by-situation, and do not treat any engine as the target or oracle. Every conclusion must be derivable from Seaborg's own instrumentation. A single coarse external EBF/depth sanity check is permitted as a reality check but must not drive the findings. Known techniques from strong engines may inform hypotheses, but each proposed experiment stands on our own measured rationale.

Measure Seaborg's own selectivity profile on a representative suite at fixed time and fixed nodes, e.g.: effective branching factor and depth reached; the move index at which beta cutoffs occur (first-move-cutoff rate is the headline move-ordering signal); the LMR re-search rate (fraction of reduced scouts that beat alpha and are re-searched at full depth) and the reduction-amount distribution by depth and move count; fail-high/fail-low and re-search rates at PV vs non-PV nodes; aspiration re-search rate; quiescence node fraction and where qsearch widens; TT-move availability and hash-hit quality.

From that profile, form first-principles hypotheses about where depth is lost (e.g. reductions too timid or too aggressive, ordering weak at particular node types, qsearch too wide) and turn them into a ranked list of candidate experiments, each individually testable by self-play SPRT at a real time control with an expected mechanism stated.

Deliverable: a report of Seaborg's selectivity profile plus a prioritized backlog of first-principles selectivity experiments (candidate follow-up tickets), with methodology recorded. This task ships no permanent engine change beyond temporary instrumentation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Seaborg's own selectivity profile is measured on a representative suite at fixed time and fixed nodes and recorded (EBF, depth reached, first-move-cutoff rate, LMR re-search rate and reduction distribution, quiescence fraction, PV/non-PV and aspiration re-search rates), using only Seaborg's instrumentation
- [ ] #2 The findings identify from first principles where effective depth is lost, each backed by our own measured signal rather than a comparison to another engine
- [ ] #3 A ranked list of candidate selectivity experiments is produced, each individually testable by self-play SPRT at a real time control, with an expected mechanism stated
- [ ] #4 Any external-engine comparison is limited to a single lightweight sanity check and is explicitly not the basis for any recommendation
- [ ] #5 Methodology (suite, time and node budgets, instrumentation, hardware) is recorded for reproducibility, and no permanent engine behaviour change is shipped by this task
<!-- AC:END -->
