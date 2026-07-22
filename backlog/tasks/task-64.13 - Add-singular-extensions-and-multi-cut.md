---
id: TASK-64.13
title: Add singular extensions and multi-cut
status: To Do
assignee:
  - '@claude'
created_date: '2026-07-19 13:33'
updated_date: '2026-07-22 02:59'
labels:
  - search
  - extensions
dependencies:
  - TASK-64.1
  - TASK-51
references:
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: feature
ordinal: 76000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add singular extensions: when the transposition-table move appears to be the only move that holds a position, search it deeper. Multi-cut is the complementary case, where several moves beat beta in a reduced search and the node can be pruned instead.

This is the most sophisticated item in the programme and is sequenced last among the search features for that reason. It is also the one with the strictest structural prerequisites, which is why it is scheduled after both the search-stack refactor and the general extension and reduction work.

Mechanism. At a node with a sufficiently deep transposition-table entry, re-search the remaining moves at reduced depth with a window just below the stored score, excluding the stored move. If they all fail low, the stored move is singular and is extended. The exclusion is the structural requirement: the re-search must be able to skip one specific move, and that excluded move must be recorded per ply where the recursive call can see it. There is nowhere to record it today, which is why this depends on the search stack.

It depends on TASK-51 because singular extensions are an extension policy, and TASK-51 establishes the extension and reduction framework at search steps 16 and 17 that this builds on. Applying singular extensions to a search with no other extension mechanism would mean building that framework here instead.

An interaction to watch: the re-search runs at the same node and shares its transposition-table slot. The stored entry that triggered the singular test must not be overwritten by the re-search in a way that invalidates the test, and no re-search may publish an entry for the node under its artificial window. The TASK-46 guarantee that incomplete subtrees cannot publish scores is the relevant precedent.

Multi-cut may be delivered with this or deferred with a recorded decision; it reuses the same reduced re-search and is conventionally cheap to add once the exclusion mechanism exists.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A move can be excluded from a search at a given ply, and the exclusion is visible to the recursive call
- [ ] #2 Singular extensions are applied under documented depth and bound conditions and are disabled at the root
- [ ] #3 The singular re-search cannot overwrite or corrupt the transposition-table entry that triggered it, and publishes no entry for the node under its artificial window
- [ ] #4 Multi-cut is implemented, or a decision to defer it is recorded with rationale
- [ ] #5 The reported principal variation remains legal under extension, verified against the TASK-36 regression coverage
- [ ] #6 Measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Hoist the stored score/bound/depth out of the TT snapshot before Step 4 consumes it, so the move loop can read them.
2. Read the per-ply excluded move at node entry (do NOT clear it there — the singular re-search re-enters the same ply and must still see it).
3. Skip the excluded move in the move loop; do not count it toward move_count. When move_count==0 under an active exclusion, return the incoming alpha (fail-low = the sole move is singular) instead of a fabricated mate/stalemate.
4. Skip the Step 4 TT cutoff and the Step 24 TT store whenever an exclusion is active (keeps the triggering entry intact and publishes nothing under the artificial window).
5. At Step 16, for the TT move only, run a reduced-depth null-window re-search around (stored_score - margin) with that move excluded, at the same ply. Gate: non-root, no active exclusion, depth>=SINGULAR_MIN_DEPTH, stored bound Lower/Exact at depth>=depth-margin, stored score is centipawns (not mate). If every other move fails low -> extend by one ply (singular). Reset the exclusion slot before propagating an abort.
6. Multi-cut: when the reduced window already sits at or above beta and the re-search still beats it, several moves hold, so return that bound as a fail-high (non-PV nodes only).
7. Restore this node's current move/piece in the stack after the re-search, which reused the slot.
8. Put singular+multi-cut behind compile-time toggles (matching the LMR toggle convention) and under extensions_enabled() so the existing exact-depth ablation tests stay valid.
9. Tests: exclusion is visible and skips the move (AC#1); the excluded re-search neither overwrites the triggering entry nor publishes one (AC#3); PV stays legal at a singular-active depth (AC#5, plus existing PV/horizon regressions). 
10. Run cargo fmt/clippy/test, then the TASK-27 strength script (baseline = merge-base) and record results (AC#6).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation (target f4f0fad)

Singular extensions added to the main search, with the multi-cut mechanism present but gated off.

Mechanism:
- Per-ply move exclusion (StackEntry.excluded), read by the recursive re-search of the same node and honoured in the move loop.
- At a node whose TT move carries a Lower/Exact bound at depth >= depth-3 with a centipawn score, the node is re-searched one move short (that move excluded) at depth (depth-1)/2, against a null window at stored_score - 4*depth cp. All others failing low => the move is singular and its subtree is extended one ply. Disabled at the root; honours the runtime extensions_enabled gate; behind compile-time SINGULAR_EXTENSIONS.
- The excluded re-search suppresses both its Step 4 TT cutoff and its Step 24 TT store, so the artificial window neither reuses nor overwrites the entry that triggered it.
- Runaway guard: a chain of singular extensions never lowers depth (new_depth == depth for the extended move), so a path-extension budget refuses further extension once ply + depth - root_depth reaches SINGULAR_MAX_PATH_EXTENSION (3). Prior art is Stockfish's ss->doubleExtensions path counter; this computes the same net path quantity via an identity instead of a stack field (net of reductions, a deliberate minor deviation).
- Multi-cut implemented on the same re-search but SINGULAR_MULTICUT=false by default: it is an unmeasured forward prune, so enabling it needs its own strength run. This is the recorded deferral for AC #4.

Tests added: an_excluded_move_is_skipped_by_the_search_at_its_ply (AC#1); a_singular_re_search_neither_publishes_nor_overwrites_the_triggering_entry (AC#3); a_singular_depth_search_reports_a_legal_pv (AC#5, alongside the existing TASK-36 PV-legality and past-horizon regressions).

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (engine lib 407 passed, 2 ignored, ~65s; back at baseline after adding the path-extension budget — an early un-budgeted build blew gives_correct_answers up to 160s and also failed it)

## Strength measurement (AC#6) — STOPPED, inconclusive

TASK-27 script, authoritative SPRT, candidate f4f0fad vs baseline (merge-base) 108c2bd.
- Runner: fastchess alpha 1.5.0; tc=8+0.08; 64 MB hash; concurrency 4; openings-v1.epd; target-cpu=native release, rustc 1.97.1; Apple M3 Pro.
- SPRT elo0=-5, elo1=0, alpha=0.05, beta=0.05 (no-regression gate).
- Stopped by request before a boundary was crossed. Partial: candidate W-D-L 92-174-94 over 360 games; score 0.497; point estimate about -1.9 Elo; ~95% CI [-38, +34] (per-game normal approx). No boundary crossed => no PASS/FAIL verdict.
- Reading: no measurable strength effect at this sample. Not a demonstrated gain and not a demonstrated regression. The tuning constants (margin 4*depth, path cap 3, depth floor 8) were conservative unmeasured choices.

## Decision needed (why Needs Human)
The mechanism is complete, correct, and green on all required checks, and AC#1-#5 are satisfied. AC#6 was measured but the feature shows no strength benefit as tuned, and the run was stopped inconclusive by request (deferring tuning). Merging a search feature that earns no measured Elo, on by default, is a product call rather than an automation call. Options on revisit: (a) tune margin/cap/depth-floor and re-measure to seek a gain; (b) land the mechanism with SINGULAR_EXTENSIONS defaulted off pending that tuning; (c) drop it. Branch and worktree left intact at f4f0fad for whichever path is chosen.

## Closed without merging (human decision)

Closed by human review and taken off the board. Note the deviation from the usual meaning of Done in this repo: the work was NOT merged. master remains at 108c2bd with none of this change; the implementation lives only on branch task-64.13-singular-extensions (code target f4f0fad).

Reason: the mechanism is complete, correct, and green on all required checks (AC#1-#5), but AC#6 measured no strength benefit and the SPRT was stopped inconclusive at 360 games (candidate 92-174-94, point estimate ~-1.9 Elo, ~95% CI [-38,+34]). Landing an unverified strength change was judged not worthwhile, and tuning was deferred.

If singular extensions are revisited, start from branch task-64.13-singular-extensions rather than from scratch: the excluded-move mechanism, the path-extension runaway guard, the TT-publication guarantees, and the tests are all in place and passing. The open work is tuning (margin 4*depth, path cap 3, depth floor 8) plus a decisive strength run, and optionally enabling SINGULAR_MULTICUT.

## Reopened to To Do (supersedes the close above)

The close-as-Done above is superseded: the ticket is back on the board as To Do. Nothing was delivered — master carries none of this change — so the task is not complete and remains eligible for a future attempt. The notes above are retained deliberately as the record of the first attempt.

Starting point for whoever picks this up: branch task-64.13-singular-extensions, code target f4f0fad. Already done and passing there: the per-ply excluded-move mechanism, the singular test and its documented gates, the transposition-table publication guarantees, the path-extension runaway guard, and the three new tests plus the existing TASK-36 PV-legality regressions. Required checks were all green on that branch.

Remaining work is strength, not mechanism: tune the constants (SINGULAR_MARGIN_PER_DEPTH=4, SINGULAR_MAX_PATH_EXTENSION=3, SINGULAR_MIN_DEPTH=8), run a decisive SPRT rather than a truncated one, and decide on SINGULAR_MULTICUT (implemented, currently off). The first attempt's run was stopped inconclusive at 360 games with no measurable benefit, so treat a decisive measurement as the gate for landing it enabled.
<!-- SECTION:NOTES:END -->
