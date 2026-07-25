---
id: TASK-80
title: 'Contract move time on stable, flat positions (symmetric stability scaling)'
status: In Review
assignee:
  - '@george'
created_date: '2026-07-24 10:34'
updated_date: '2026-07-25 11:20'
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

MEASUREMENT PIVOT (with user): equal-speed self-play cannot surface the flagging benefit (the loss happens vs a faster field), and the mandated harness fail-closes on ANY time forfeit as an infrastructure error (docs lines 118-121; forfeit-counting is a reserved-but-unbuilt mode). So the no-increment flagging regime is unmeasurable with the harness as-built. Per user decision, dropped the no-increment Elo claim; validating via (a) direct mechanism evidence and (b) an incremented no-regression gate.

BUG FOUND + FIXED (commit e6b8117): the first cut (5272383) barely engaged. stability_scale entangled extension and contraction — any positive score_drop pushed scale>1 and short-circuited past the contraction branch. A genuinely flat search wobbles a few cp between iterations, so ~half the iterations of every flat position read the wobble as a 'fall' and vetoed contraction. Aggregate over 2699 self-play games showed candidate vs baseline mean move-time 99.3 vs 99.4ms (no effect); per-move A/B on a dead-drawn KP endgame showed candidate spent the FULL budget to the same depth as baseline. Fix: make the two directions mutually exclusive on the settled streak the caller already maintains (contract iff stable_iterations>0, else extend exactly as before). A sub-margin wobble is the flat case, not a fall.

MECHANISM EVIDENCE (fixed candidate, controlled per-move A/B, go wtime 40000 movestogo 20): dead-drawn KP endgame 8/5pk1/6p1/4P1P1/5PK1/8/8/8 -> baseline 24 plies/2691ms vs candidate 23 plies/494ms (-82% time, banking the rest of the clock). Sharp tactical positions do not contract (extend as before). Contraction scales with search depth x settledness, concentrating on deep dead-flat searches = the drawish-endgame archetype of the motivating loss. Deterministic unit tests cover onset gating, monotone contraction, floor, and the settled/unsettled branch invariant.

Non-regression gate: incremented tc=10+0.1 SPRT (elo0=-5 elo1=0), baseline b2f9457 vs candidate e6b8117, running now.

FINAL STATE — implementation target be78d54 (base b2f9457).

Reviewer context on acceptance criteria:
- AC#1/AC#3/AC#4: satisfied by stability_scale + SoftLimit::deadline. Contraction below 1.0 only when root move held AND |inter-iteration score delta| <= STABILITY_FLAT_MARGIN across consecutive iterations; floored at MIN_STABILITY_SCALE=0.5; hard/maximum deadline, guaranteed first ply, and legal-bestmove path untouched; derived only from root-move stability + inter-iteration score delta (no node-fraction, no cross-move memory).
- AC#2 nuance to check deliberately: the extension direction is byte-identical for genuinely unstable iterations (changed root move, or a score fall beyond the flat margin). ONE intended behavioural change: a settled iteration (move held, score within the +/-8cp flat margin) no longer buys the trivial <=8/150 'score_drop' extension it did before — it is now treated as flat and eligible to contract. This is required: without it, ordinary sub-margin eval wobble vetoes contraction on ~half the iterations of every flat position (that was the first-cut bug, fixed in e6b8117). So 'unstable extends exactly as today' holds for real instability; sub-margin drifts are reclassified as flat by design.
- AC#5: PARTIAL by explicit maintainer decision. Round-robin SPRT structurally cannot measure this change's benefit (equal-speed self-play removes the fast-field flagging asymmetry; the harness fails closed on any time-forfeit, so the no-increment regime cannot be scored). Recorded in BENCHMARKS.md as mechanism-based + non-regression. Mechanism: dead-drawn KP endgame 8/5pk1/6p1/4P1P1/5PK1/8/8/8, go wtime 40000 movestogo 20 -> baseline 24 plies/2691ms vs candidate 23 plies/494ms (-82%, banks the clock); sharp positions still extend. Non-regression gate (tc=10+0.1, elo0=-5 elo1=0): environment-limited; session teardowns repeatedly reaped the match; partial windows (2409 games @ 0.4952 ~-3 Elo; 457 games @ 0.5066 ~+5 Elo; 0 forfeits) agree near-neutral, no trend toward regression. No single window reached a boundary or the cap. Merged as low-risk mechanism-sound per maintainer decision.
- AC#6: cargo fmt --check clean; cargo clippy --workspace --all-targets --all-features -D warnings clean; cargo test --workspace all pass (engine lib 430 + 57+157+19+6+1... ; 0 failed).

Binaries used for measurement (outside repo): /Users/seabo/seaborg-builds/seaborg-task80-{baseline,candidate}. Candidate = e6b8117 code (identical to be78d54 minus the BENCHMARKS/task-file commit).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-25 11:19
---
Implementation handoff
Branch: task-80-symmetric-stability-scaling
Worktree: /Users/seabo/seaborg-worktrees/task-80-symmetric-stability-scaling
Base: b2f9457
Implementation target: be78d54
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (engine lib 430; all other binaries pass; 0 failed)
Known failures: none. (A pre-existing wall-clock test, an_extendable_budget_is_still_bounded_by_its_hard_half, can flake under extreme full-workspace CPU load when the search thread is descheduled past its hard deadline; it passed in the final clean run and is diff-independent — the change only alters scale<1 behaviour, and that test uses an extending scale>=1 position where SoftLimit::deadline is byte-identical to base.)
AC#5 measurement is partial by explicit maintainer decision (mechanism + non-regression); see implementation notes and BENCHMARKS.md for why the SPRT cannot score this change and the evidence gathered.
---
<!-- COMMENTS:END -->
