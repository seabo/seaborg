---
id: TASK-80
title: 'Contract move time on stable, flat positions (symmetric stability scaling)'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-24 10:34'
updated_date: '2026-07-24 11:17'
labels: []
dependencies: []
ordinal: 136000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Today all search-side time adaptivity only ever *extends* a move: `instability_scale` in engine/src/search.rs returns `scale.max(1.0)`, so a changing root move (+0.6) or a falling eval (+up to 1.0/150cp) pushes the soft limit toward `maximum`, but nothing ever pushes it *below* `optimum`. A dead-equal position where the same root move is best at every iteration and the eval is flat still spends the full planned budget every move.

This is the direct cause of a lost lichess game at no increment: in a drawish endgame with no advantage to convert, we bled our clock ~5% per move searching for a better move that did not exist, while the opponent played fast non-losing moves and won the time race. Strong engines (e.g. Stockfish) spend a *fraction* of optimum when the best move is stable and search effort is concentrated. Adding the contraction direction addresses the flag scenario at every time control, without any opponent-clock heuristic (which the strong-engine field deliberately does not use).

Scope: the contraction lever only. Make the stability signal symmetric so an obviously-settled position spends less than optimum, bounded by a sensible floor. Out of scope for this task (candidate follow-ups): best-move node-fraction plumbing, and cross-move eval memory.

Prior art: TASK-40 introduced the soft/hard split and next-iteration prediction; this extends that mechanism rather than replacing it. Allocation in engine/src/time.rs stays mechanical; all changes live on the search side as a multiplier on the soft limit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The stability scale applied to the soft limit can take values below 1.0 when the root best move has been stable across multiple recent iterations and the inter-iteration eval is flat, contracting the planned spend below optimum
- [ ] #2 A stable, flat position measurably spends less wall-clock per move than before the change at a fixed time control, and an unstable position (changing root move or falling eval) still extends exactly as it does today
- [ ] #3 The soft limit is never contracted below a documented floor, and the hard/maximum deadline is unchanged; the guaranteed-first-ply and legal-bestmove contracts are untouched
- [ ] #4 Contraction is derived only from within-search signals already available (root-move stability across iterations, inter-iteration score delta); no dependency on node-fraction or cross-move eval memory
- [ ] #5 Strength is measured in a controlled base-vs-target SPRT on the incremented fastchess harness (not a cross-session comparison) and is neutral-or-better, with attribution recorded in BENCHMARKS.md; a no-increment (sudden-death) matrix is included since that is the regime the motivating loss came from
- [ ] #6 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make the soft-limit multiplier symmetric in engine/src/search.rs. Rename instability_scale -> stability_scale; add a stable_iterations arg. Extension branch (best-move change / falling score) is unchanged, so an unstable position extends exactly as today. When neither fires, contract below 1.0 as consecutive settled iterations accumulate.
2. Add contraction constants: MIN_STABILITY_SCALE (documented floor), STABILITY_FLAT_MARGIN (|inter-iter delta| that still counts as flat), STABILITY_CONTRACTION_ONSET (settled iterations before contraction starts), STABILITY_CONTRACTION_PER_ITER (step per iteration past onset).
3. Remove the scale.max(1.0) clamp in SoftLimit::deadline that currently blocks any sub-1.0 scale; the floor now lives in stability_scale. Update its doc.
4. In iterative_deepening, track stable_iterations: increment only when a previous iteration exists, the root move held, and |score delta| <= flat margin (both directions, not only a drop); reset otherwise. Pass it into stability_scale.
5. Rename the instability local/param to a symmetric name; update next_iteration_fits doc.
6. Update existing unit tests for the new signature; add tests: contraction only after onset, monotone decrease, clamped at floor, unstable still extends unchanged, deadline honours sub-1.0 scale, first-ply/hard-deadline untouched.
7. Run fmt/clippy/test. Then controlled base-vs-target SPRT on fastchess (incremented + no-increment/sudden-death matrix); record attribution in BENCHMARKS.md.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation committed at 5272383 (base b2f9457).

Design: stability_scale (renamed from instability_scale) is now bidirectional. Extension branch unchanged, so unstable positions extend exactly as before (AC#2, AC#4). Contraction below 1.0 activates once the root move has held and |inter-iteration score delta| <= STABILITY_FLAT_MARGIN (8cp, both directions) for consecutive iterations; each iteration past STABILITY_CONTRACTION_ONSET (3) removes STABILITY_CONTRACTION_PER_ITER (0.1), floored at MIN_STABILITY_SCALE (0.5) (AC#1, AC#3). Removed the scale.max(1.0) clamp in SoftLimit::deadline that previously blocked any sub-1.0 scale; floor now lives in stability_scale. Hard deadline / guaranteed first ply / legal-bestmove untouched (AC#3).

Unit tests added: contraction waits for onset then decreases monotonically; never below floor; extension ignores prior streak; next_iteration_fits declines an iteration once contracted (mirror of the extension test).

Checks: cargo fmt --check clean; cargo clippy -D warnings clean; cargo test -p engine --lib 430 passed under heavy parallel load. One env-timing flake in an_extendable_budget_is_still_bounded_by_its_hard_half under full-workspace load (search thread descheduled past the 60ms hard deadline); passes in isolation and at base; provably diff-independent (extending position => scale>=1.0 => deadline() byte-identical to base). Final clean workspace run to be taken after SPRT (idle machine).

AC#5 SPRT: baseline=b2f9457, candidate=5272383, both target-cpu=native release locked. Sudden-death regime (tc=10+0, concurrency 4, max 4000) running now; incremented (tc=8+0.08) to follow. Attribution to be recorded in BENCHMARKS.md.
<!-- SECTION:NOTES:END -->
