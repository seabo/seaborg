---
id: TASK-64.21
title: Add SEE-based move pruning in the main search
status: To Do
assignee: []
created_date: '2026-07-21 12:56'
labels:
  - search
  - pruning
  - see
dependencies:
  - TASK-64.9
parent_task_id: TASK-64
priority: medium
ordinal: 128000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deferred half of TASK-64.9. That task set out to add static-exchange pruning in both quiescence and the main search, delivered together. Only the quiescence cuts shipped (a robust +70 Elo gain); the main-search prune was measured to be net-harmful and was removed. This task is to make main-search SEE pruning actually gain, or to conclude with evidence that it cannot in this engine.

WHAT WENT WRONG LAST TIME (TASK-64.9 measurements, fastchess nodes=100000 vs the merge-base, one sitting):
- Quiescence SEE + delta cuts ALONE: +70.4 +/- 39.4 Elo (LOS 99.99%, 200 games). Shipped.
- Main-search prune ALONE (non-PV, shallow depth, depth-scaled SEE floor; captures floor -(300+100*depth), quiets floor -60*depth; check-giving moves exempted): -19.1 +/- 40.3 Elo (LOS 17%, 200 games; not individually significant).
- BOTH cuts together: -88.7 +/- 19.9 Elo (LOS 0%, 500 games).
So the two prunes interact strongly and destructively: q-cuts alone +70, but adding the main-search prune collapses the combined change to -88 (a ~158 Elo negative interaction). The mechanism is NOT understood and was NOT the material-only-eval guess made at the time (the leaf eval is a tapered HCE / NNUE, not material-only). Understanding this interaction is the crux of the task.

TWO HARD CONSTRAINTS THAT SURFACED:
1. The capture floor is pinned near-inert by the shallow forced mates in the search regression suite (gives_correct_answers, child_mate_windows_preserve_distance_parity). A sacrificial mating capture has SEE ~ -300; pruning captures losing less than ~a minor piece reverts those mates to a bare material score. So the capture floor had to be kept a minor piece deep (-(300+100*depth)), which prunes almost nothing. Net: the main-search prune's only real effect was the QUIET-move prune.
2. Raw depth-scaled quiet SEE pruning (fire in all non-PV nodes at depth<=6 with floor -60*depth) is the harmful part and does not compose with the q-cuts.

SUGGESTED DIRECTION (not a committed plan; the worker should research current code first):
- Match Stockfish-style gating rather than a raw depth floor: scale the quiet SEE threshold by lmrDepth (the LMR-reduced depth) not raw depth, and gate on move ordering / history so only late, low-history quiets are cut. Consider capture SEE pruning gated on depth and move count with a floor that still respects the mate suite.
- ALWAYS measure in combination with the shipped quiescence cuts (the current master behaviour), never in isolation, because the interaction is where the loss lives. Use the TASK-27 harness against the appropriate baseline.
- If a properly-gated version still cannot beat the q-cuts-only baseline, close the task with that negative result recorded rather than shipping a regression.

Reference commit for the removed approach and full analysis: the TASK-64.9 implementation notes and its target commit on branch task-64.9-see-pruning.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A main-search SEE prune is added and, combined with the existing quiescence cuts, is measured with the TASK-27 harness to be non-negative versus the quiescence-cuts-only baseline (or the task is closed with a recorded negative result showing it cannot beat that baseline)
- [ ] #2 The shallow forced mates in the search regression suite (gives_correct_answers, child_mate_windows_preserve_distance_parity) still pass
- [ ] #3 The prune is confined to non-PV nodes, never fires while in check, always searches the first move, and exempts or otherwise protects checking/sacrificial moves so tactics are preserved
- [ ] #4 Strength measured in COMBINATION with the quiescence cuts (not in isolation), with the interaction between the two prunes explicitly characterised in the implementation notes
- [ ] #5 Quiescence node counts / share and main-search node counts reported before and after on a representative position set
<!-- AC:END -->
