---
id: TASK-64.22
title: >-
  Refine late move reduction with a log-based table, history modulation, and
  node-type adjustments
status: To Do
assignee: []
created_date: '2026-07-21 21:22'
updated_date: '2026-07-21 22:23'
labels:
  - search
  - strength
dependencies:
  - TASK-51
parent_task_id: TASK-64
priority: medium
ordinal: 130000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The late move reduction landed in TASK-51 is correct but coarse: `lmr_reduction` (engine/src/search.rs) is a hand-tuned step function returning 1 ply, or 2 only when `depth >= 8 && move_count >= 8`. It ignores the move-ordering signals the engine already computes, so late moves deep in a long quiet list are under-reduced and the reduction never scales with how promising a move actually is.

The infrastructure needed to modulate the reduction is already merged: main history (TASK-64.2), counter-move and continuation history (TASK-64.10), and the improving signal (TASK-64.12). This task spends that signal on the reduction amount to widen effective search depth without strength loss.

Scope: (1) replace the step function with a precomputed reduction table indexed by remaining depth and move count, growing roughly like a log(depth)*log(move_count) curve; (2) modulate the base reduction by the moving side accumulated quiet history (main + continuation) so well-scored quiets reduce less and poorly-scored quiets reduce more; (3) reduce one extra ply when the side to move is not improving; (4) reduce less on PV nodes and for killer/counter moves so the ordering prefix keeps its depth. Preserve every existing safety property from TASK-51: the first move and moves that give check or receive an extension are never reduced, the reduced scout always keeps at least one ply, and any reduced scout that beats alpha is re-searched at full depth before it can enter the PV.

Out of scope (defer): cut-node-specific reduction schemes and TT-capture-driven adjustments, which pair with singular extensions (TASK-64.13) and are kept separate to preserve clean strength attribution.

Measurement discipline: each refinement must be individually gated so its strength contribution can be isolated, and net strength must be confirmed by a round-robin base-vs-target match at a real time control (not a fixed node budget, which inflates search-pruning changes), with the result and attribution recorded in BENCHMARKS.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coarse step-function reduction is replaced by a precomputed table indexed by remaining depth and move count that grows monotonically in both
- [ ] #2 The applied reduction is decreased for quiet moves with strong accumulated history (main and continuation) and increased for weak history
- [ ] #3 A non-improving side to move receives an additional ply of reduction; an improving one does not
- [ ] #4 PV nodes and killer/counter moves receive less reduction than a plain late quiet move at the same depth and move count
- [ ] #5 All TASK-51 safety properties still hold: move one, checking moves, and extended moves are never reduced; the reduced scout keeps at least one ply; and every reduced scout that raises alpha is re-searched at full depth before populating the PV, verified by the existing TASK-36 PV-legality and TASK-51 soundness tests
- [ ] #6 Each refinement is independently toggleable so its individual effect can be measured
- [ ] #7 Net strength is confirmed by a round-robin base-vs-target match at a fixed time control showing no regression, with results and attribution recorded in BENCHMARKS.md
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-21 22:23
---
Revisit flag from TASK-64.21 (main-search SEE pruning, closed as a negative result): a properly-gated Stockfish-style main-search SEE prune measured NO gain at a fair time control (-17 +/- 31 Elo; the +137 at nodes=100000 was a node-budget artifact). Diagnostics showed the prune is well-targeted (flagged quiets cut off at 1.35% vs a 4.03% baseline) but LOW-LEVERAGE precisely because the current lmr_reduction this task replaces is nearly a no-op (lmr_depth ~ raw depth), so the lmrDepth-scaled prune is inert. Once this LMR refinement lands (aggressive, history/depth-scaled reductions), main-search SEE pruning becomes worth re-measuring — its leverage in Stockfish comes from exactly that kind of LMR plus continuation-history pre-filtering. Suggestion for whoever picks up this task: after it merges, file a fresh ticket to re-attempt main-search SEE pruning; the prior implementation + mechanism diagnostics live in branch task-64.21-main-search-see-pruning (target 2353acb).
---
<!-- COMMENTS:END -->
