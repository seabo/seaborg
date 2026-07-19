---
id: TASK-64.1
title: Track ply explicitly and introduce a per-ply search stack
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-19 13:30'
updated_date: '2026-07-19 16:38'
labels:
  - search
  - architecture
  - refactor
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/pv_table.rs
  - engine/src/killer.rs
parent_task_id: TASK-64
priority: high
type: enhancement
ordinal: 64000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ply-from-root is currently derived by subtracting remaining depth from the iteration depth. Replace that derivation with an explicit ply index, make depth signed, and introduce a per-ply search stack. This is a hard prerequisite for reductions, extensions and singular extensions; it is not a cleanup.

The assumption and where it breaks. search.rs:646 computes `let draft = self.search_depth - depth`, and PVTable derives its row identically as `k = m - d` (pv_table.rs:55-57 in clear_at, :70-72 in update_internal). Both are correct only while depth decreases by exactly one per recursion, which makes depth a stand-in for ply. Two consequences once that stops holding:

- Any extension makes depth exceed search_depth. `self.search_depth - depth` is a u8 subtraction, so it panics in debug and wraps in release; PVTable::clear_at then indexes `data[(k * m)..(k * m + d)]` with a wrapped k, which panics or corrupts the reported variation.
- Any reduction makes two sibling subtrees at the same ply carry different depth. They store killers at different draft indices and write different PV rows, and a parent reads the row its own child did not write. This is the same class of defect TASK-36 already had to repair once.

Consumers of the derived value today are the killer table (`kt.store(*mov, draft)` at search.rs:897, `kt.probe(self.draft, ...)` at search.rs:1457) and PVTable (`clear_at(depth)` at search.rs:652, `copy_to(depth, *mov)` at search.rs:886).

Depth signedness. depth is u8, so `depth - 1` is already a boundary case and `depth - 1 - r` for a reduction r cannot be expressed without underflow. A signed depth that is allowed to fall to or below zero, with quiescence entered on that condition, removes a class of arithmetic hazard from every future reduction.

Search stack. Per-ply state is presently spread across three structures with three indexing conventions, and several planned features need state that has nowhere to live. A stack indexed by ply is the conventional home for: the static evaluation at that ply, the move played, the excluded move that singular extensions must exclude from the re-search, the continuation-history pointer, and the PV row. Introducing it now avoids threading each of these separately later.

Quiescence takes no ply argument at all, which is why TASK-29 exists. Giving quiescence a ply as part of this work makes that cap expressible; implementing the cap itself remains TASK-29.

This refactor should be behaviour-preserving. The search test suite in search.rs is extensive and covers mate parity, PV legality, abort behaviour and transposition-table reuse near the fifty-move boundary; it is the safety net that makes this change tractable and should pass unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Ply from root is passed explicitly through the main search and quiescence rather than derived from search depth, and no code computes ply by subtracting depth from iteration depth
- [ ] #2 Search depth uses a signed type and may be reduced to or below zero without underflow, entering quiescence on that condition
- [ ] #3 A per-ply search stack exists with slots for at least static evaluation, the move played, and an excluded move, and is indexed by ply
- [ ] #4 Killer moves are indexed by ply and remain correct when siblings at the same ply are searched at different depths
- [ ] #5 The principal variation is indexed by ply, and a node whose depth exceeds the nominal iteration depth neither panics nor writes another ply row
- [ ] #6 Quiescence receives a ply argument, whether or not a cap is applied to it
- [ ] #7 A regression test exercises a node searched at greater depth than its nominal iteration depth and asserts no panic and a legal reported principal variation
- [ ] #8 The existing search test suite passes without modification to its assertions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add `Depth = i16` and `MAX_PLY` to search.rs; thread an explicit `ply: usize` through `search`/`search_inner`/`quiesce`/`quiesce_inner`/`quiesce_evasions`, root ply 0, child ply+1. Delete `search_depth` and the `draft = search_depth - depth` derivation.
2. Make depth signed: `should_razor`, recursion (`depth - 1`), Step 5 becomes `depth <= 0 -> quiescence`, TT depth comparisons widen to `Depth`, TT stores clamp back into the u8 draft field.
3. Introduce `SearchStack`/`StackEntry` (static eval, move played, excluded move) owned by `Search` and indexed by ply; razoring reads the stored eval, the move loop records the move played. Cap main-search ply at `MAX_PLY` by diverting to quiescence.
4. Re-index `KillerTable` by ply (direct `data[ply]`, root slot unused) preserving the existing 20-ply killer reach.
5. Rewrite `PVTable` to be ply-indexed and stored in forward order: row `ply` holds the line from that ply, `clear_at`/`copy_to` no-op above the nominal ply count so an extended node neither panics nor writes another row. Update pv_table unit tests to ply semantics and the Debug renderer.
6. Add regression tests: a node searched deeper than its PV table's nominal depth (no panic, legal PV); killer store/probe by ply independent of depth; quiescence reached at a ply below the nominal horizon leaves no stale row.
7. Run cargo fmt --check, clippy -D warnings, cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Ply is now a parameter of `search`/`search_inner`/`quiesce`/`quiesce_inner`/`quiesce_evasions`, root ply 0. `Search::search_depth` and the `search_depth - depth` derivation are deleted outright, so nothing can reintroduce the subtraction.

Depth is `pub type Depth = i16`. Step 5 tests `depth <= 0` rather than `== 0`, since a reduction can cross zero in one step. The transposition table's draft field is still a byte, so `Search::tt_draft` narrows on store; it saturates downwards, which understates how deeply an entry was searched and costs hit rate rather than soundness.

`StackEntry` (static eval, move being searched, excluded move) is a `pub` struct with `pub` fields, held as `Box<[StackEntry; MAX_PLY]>`. Public because the excluded-move slot has no reader yet — singular extensions are later work — and a private write-only field would be a `dead_code` failure under strict Clippy. Its doc records the constraint a future user must also honour: an excluded re-search must be kept out of the transposition table, since its value describes a restricted move list rather than the position. A node with no room for a child (`ply + 1 >= MAX_PLY`) hands over to quiescence, which is what lets every stack index in the node body be unguarded.

`PVTable` was rewritten rather than patched: rows are indexed by ply and stored in forward order, so `pv()` reads row 0 left to right instead of the previous reversed depth-derived layout. `clear_at`/`copy_to` are no-ops above the nominal ply count, which is how an extended subtree neither panics nor writes another ply's row. The deepest row holds exactly one move, so it never reads a child row that does not exist. The pv_table unit tests were rewritten for ply arguments; their assertions are unchanged in substance.

`KillerTable` indexes `data[ply]` directly. Sized `KILLER_PLIES = 21` so the reach is exactly the plies 1..=20 the old `data[draft - 1]` with size 20 covered — deliberately not widened, to keep the change behaviour-preserving.

Quiescence receives a ply but does not index per-ply state with it, because nothing bounds the quiescence tree yet. Threading the value is what makes the cap expressible later.

Behaviour preservation was measured, not assumed: fixed-depth UCI searches (`go depth 8`) on startpos, Kiwipete and `8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1` produce byte-identical `info` lines — node counts, scores and PVs at every iteration — against the base commit c55508b. No search test assertion was modified; only call sites gained the ply argument and the deleted `search_depth` setup lines were dropped.

Observed but not changed, as it is outside this task: `KillerTable::store` compares slot hit counts with `<`, so with both counters at zero it always writes slot B. Slot A is only reached after a probe has incremented B's counter. Pre-existing on master and unaffected by the re-indexing.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 16:38
---
Implementation handoff
Branch: task-64.1-explicit-ply-search-stack
Worktree: /Users/seabo/seaborg-worktrees/task-64.1-explicit-ply-search-stack
Base: c55508b3383577ed9bb62a9ebadb21fc3ecedc1f
Implementation target: ea17ad7
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 250 passed / 0 failed / 2 ignored (engine lib), all other targets green
- behaviour-preservation probe: fixed-depth 'go depth 8' UCI searches on startpos, Kiwipete and 8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1 produce info lines identical to base c55508b (node counts, scores and PVs at every iteration)
New tests: search::tests::a_node_searched_past_the_nominal_horizon_still_reports_a_legal_pv, search::tests::a_depth_reduced_below_zero_hands_over_to_quiescence, three killer::tests ply-indexing tests, two pv_table::tests covering plies beyond the table
Known failures: none
---
<!-- COMMENTS:END -->
