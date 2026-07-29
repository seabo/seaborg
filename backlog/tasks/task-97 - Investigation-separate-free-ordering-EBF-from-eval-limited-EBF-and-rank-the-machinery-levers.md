---
id: TASK-97
title: >-
  Investigation: separate free (ordering) EBF from eval-limited EBF and rank the
  machinery levers
status: To Do
assignee: []
created_date: '2026-07-29 18:44'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
  - tools/diag/selectivity_profile.py
  - BENCHMARKS.md
  - AGENTS.md
priority: high
type: spike
ordinal: 165000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella investigation continuing the TASK-88 / TASK-89 / TASK-91 selectivity line. Organizing principle: separate the EBF gap into "free" EBF (recoverable by move ordering, off the eval flywheel) versus "eval-limited" EBF (on the flywheel), quantify the prize per ply so we know what a win is worth, then rank the specific machinery levers. Everything is scored by first-move-cutoff rate and self-play SPRT — never by EBF-at-fixed-nodes as a target.

Philosophy (see AGENTS.md / CLAUDE.md). This is NOT a gap-closing exercise against another engine. Every conclusion must be derivable from Seaborg's own instrumentation; a single coarse external EBF/depth sanity check is permitted but must not drive findings. Known techniques may inform hypotheses; each experiment stands on our own measured rationale.

Baseline numbers to compare against (this session, bench-positions.epd, 64 MB hash):
- Fixed depth 14: EBF 2.71, first-move-cutoff 88.5%, TT-move-avail 27.5% (non-PV 27.0%); cutoff rate with TT move 92.7% vs without 86.4%; no-TT nodes = 67% of all cutoffs.
- Fixed 2M nodes: EBF 2.47, depth 16.25, first-move-cutoff 89.4%, TT-move-avail 35.4% (non-PV 31.4%).
- Re-baseline (BENCHMARKS.md): geomean EBF ~2.30 vs a frontier ~2.01; effective depth ~17 vs ~23 at 1500 ms; NPS at parity.

Structure. This umbrella has one subtask per plan item across four tracks:
- Track A (locate the free EBF): A2 first-move-cutoff decomposition (reuse selstats), A1 oracle-ordering ceiling (the single most important measurement).
- Track B (fixable vs inherent): B1 TT-size sweep, B2 replacement-policy A/B.
- Track C (quantify the prize): C1 Elo-per-ply, C2 eval-exploitation cross-check.
- Track D (rank the machinery levers): D1 ordering-component ablation, D2 budgeted singular extensions, D3 IIR vs a move-synthesizing alternative at no-TT nodes.

Guardrails (so this does not re-circle):
1. Ordering health = first-move-cutoff rate, not EBF-at-fixed-nodes. EBF is the readout we are explaining, never the objective.
2. Every candidate that ships a mechanism clears an SPRT at tc=10+0.1 on the rig (24 cores, self-play only).
3. Sequence: run A2, then A1 (the ceiling) and B1 (fixable?) and C1 (the prize) first — those three decide everything. Then C2, then Track D by whatever A/B pointed to. Do not touch pruning constants or net scaling until A1 says how much free EBF exists.
4. Falsifier for the whole thesis: if A1 shows EBF_real is about EBF_oracle (little ordering waste) AND C1 shows a ply is cheap, then the off-flywheel lever is small and the hypothesis is wrong — return to the eval flywheel. Make that outcome explicit so the thread can kill the hypothesis cleanly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All nine subtasks (A1, A2, B1, B2, C1, C2, D1, D2, D3) are created, scoped, and linked under this parent
- [ ] #2 Findings from each subtask are recorded such that the free-vs-eval-limited EBF split (Track A), the fixable-vs-inherent question (Track B), and the Elo-per-ply prize (Track C) are answered before any Track D machinery ships
- [ ] #3 The thesis is resolved explicitly: either a ranked, SPRT-gated set of ordering/machinery levers is produced, or the falsifier in guardrail 4 fires and the investigation hands back to the eval flywheel with the evidence stated
<!-- AC:END -->
