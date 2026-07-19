---
id: TASK-64.1
title: Track ply explicitly and introduce a per-ply search stack
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-19 13:30'
updated_date: '2026-07-19 18:40'
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
- [x] #1 Ply from root is passed explicitly through the main search and quiescence rather than derived from search depth, and no code computes ply by subtracting depth from iteration depth
- [x] #2 Search depth uses a signed type and may be reduced to or below zero without underflow, entering quiescence on that condition
- [x] #3 A per-ply search stack exists with slots for at least static evaluation, the move played, and an excluded move, and is indexed by ply
- [x] #4 Killer moves are indexed by ply and remain correct when siblings at the same ply are searched at different depths
- [x] #5 The principal variation is indexed by ply, and a node whose depth exceeds the nominal iteration depth neither panics nor writes another ply row
- [x] #6 Quiescence receives a ply argument, whether or not a cap is applied to it
- [x] #7 A regression test exercises a node searched at greater depth than its nominal iteration depth and asserts no panic and a legal reported principal variation
- [x] #8 The existing search test suite passes without modification to its assertions
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

author: @claude
created: 2026-07-19 18:40
---
Review verdict: APPROVED

Implementation target: ea17ad7 (immutable; the only later commit, 9705ed5, touches the task file alone, and `git diff ea17ad7 HEAD -- ':!backlog'` is empty)
Base: c55508b
Branch/worktree: task-64.1-explicit-ply-search-stack, /Users/seabo/seaborg-worktrees/task-64.1-explicit-ply-search-stack

Repository-required checks, run on the target rather than trusted from the handoff:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: exit 0, zero warnings. Re-run with a clean CARGO_TARGET_DIR to rule out a cached result; engine relinted from scratch, still clean. No new #[allow] anywhere in the diff.
- cargo test --workspace: pass, 250 engine tests / 0 failed / 2 ignored. All seven new tests observed running and green.

Acceptance criteria, all proven:
1/6. Signatures thread `ply` through search/search_inner/quiesce/quiesce_inner/quiesce_evasions from root 0. `grep -rn search_depth engine/src/` returns only an unrelated test name, so the subtraction cannot be reintroduced.
2. `a_depth_reduced_below_zero_hands_over_to_quiescence` drives depths 0, -1 and -7 and asserts each equals a pure quiescence search.
3. `StackEntry` carries eval, mov and excluded, held as `Box<[StackEntry; MAX_PLY]>` and indexed by ply; written on every node the suite exercises.
4. Three killer tests cover same-ply sharing, non-leakage to neighbouring plies, and the root/out-of-reach cases. Depth is no longer an input to the table at all, so the sibling-depth hazard is structurally gone.
5/7. `a_node_searched_past_the_nominal_horizon_still_reports_a_legal_pv` searches at depth 6 against a table sized for 3, asserting no panic, pv.len() <= 3 and full PV legality; `plies_beyond_the_table_neither_panic_nor_disturb_the_reported_line` covers the table directly. Confirmed non-vacuous: base has `self.search_depth - depth` (search.rs:674, u8 underflow) and `clear_at`'s `k = m - d` (usize underflow), so this scenario genuinely panicked before.
8. The search-suite diff is confined to call sites gaining a ply argument and deleted `search_depth` setup lines. No assertion changed.

Independent correctness checks:
- `PVTable::copy_to` bounds hold at the worst case ply = plies-2 (plies^2 - plies + 1 <= plies^2); the deepest row has len 1, so it never reads a child row that does not exist.
- The old-to-new index remapping is exact: killer draft d -> data[d-1] with size 20 becomes ply d -> data[d] with size 21, preserving reach over plies 1..=20.
- `should_razor`'s `252 * depth * depth` cannot overflow i16 on a negative depth because Step 5 (depth <= 0 -> quiescence) precedes Step 7, so it only ever sees 1..=6.
- `tt_draft` saturates downwards and is identity over the reachable range, and Step 4's `Depth::from(e.depth()) >= depth` is identity for depth >= 1, so TT behaviour is unchanged.

Behaviour preservation, measured: base built in a throwaway worktree and depth-8 searches run through both binaries on startpos, Kiwipete and 8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1. Node counts, scores, PVs and hashfull are identical at every iteration; only time/nps differ.

Performance: round-robin `cargo bench --bench search`, three rounds alternating base and target. Taking the minimum per configuration as BENCHMARKS.md prescribes, base 40.030 / 39.499 us against target 40.948 / 40.402 us, i.e. +2.3% on both configurations. That is below the documented 42.26 us investigate threshold and inside the ~3% run-to-run drift band BENCHMARKS.md records for this machine, which was not idle during measurement. The deadline gap, the figure that document says to watch, is preserved: 0.53 us at base against 0.55 us at target, so the TASK-41 clock throttle is intact. Recorded here for attribution rather than treated as a regression; the cost is consistent with the two per-node stack writes this task exists to introduce.

Non-blocking observations, not defects and deliberately not filed as follow-up tasks:
- The `ply + 1 >= MAX_PLY` handover returns before `clear_at(ply)`. That is safe only because `PVTable::new` takes a u8, so plies <= 255 < MAX_PLY and the clear would no-op regardless. Raising MAX_PLY or widening PVTable would turn the skipped clear into a stale-row bug.
- That same handover counts the node through both `visit_node` and `visit_q_node`. Cosmetic, and unreachable without an iteration depth near 255.
- `stack[ply].eval` currently has no reader; razoring still uses the local. Expected, since the slot exists for later work.

Comment quality: no comment in the diff cites a task ID, acceptance criterion, review finding ID or Backlog document, and the added comments state reasons rather than restating code.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the depth-derived ply with an explicit ply index threaded through the main search and quiescence, made depth a signed `Depth = i16` that enters quiescence at `depth <= 0`, and added a `MAX_PLY` per-ply `SearchStack` (static eval, move played, excluded move). `KillerTable` and `PVTable` are now ply-indexed; the PV table stores rows in forward order and no-ops above its nominal ply count, so an extended subtree neither panics nor writes another ply's row. `Search::search_depth` and the `search_depth - depth` subtraction are deleted outright.

Verified at ea17ad7: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings (exit 0, zero warnings, re-confirmed with a clean CARGO_TARGET_DIR), and cargo test --workspace (250 engine tests pass, including the seven new ply-indexing and past-horizon regression tests). Behaviour preservation was measured rather than assumed: depth-8 UCI searches on startpos, Kiwipete and 8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1 produce identical node counts, scores, PVs and hashfull at every iteration against base c55508b. Round-robin search benchmarking over three rounds shows +2.3%, below the 5% BENCHMARKS.md gate and inside that document's stated ~3% drift band, with the deadline gap preserved at ~0.5 us.
<!-- SECTION:FINAL_SUMMARY:END -->
