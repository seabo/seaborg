---
id: TASK-61
title: Add benchmark-backed transposition-table hot-path enhancements
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-19 00:01'
updated_date: '2026-07-19 20:27'
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
- [x] #1 Representative fixed-depth positions and a reproducible benchmark establish baseline nodes, elapsed time, and probe behavior before hot-path changes
- [x] #2 The value and validity conditions for a stored static evaluation are specified, including interaction with rule-sensitive evaluation from TASK-58; it is implemented only if measurements or imminent pruning consumers justify its entry-space cost
- [x] #3 Child-bucket prefetching is evaluated on supported targets and retained only if it produces a repeatable benefit without harming portability or safety
- [x] #4 Accepted enhancements include regression and benchmark coverage; rejected candidates have their measurements and decision recorded so the experiment is not repeatedly rediscovered
- [x] #5 The final entry layout remains compact and its memory footprint and cache-line organization are asserted or tested
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework: re-integrate the approved b76a0c2 work onto current primary after a stale-base merge eject (TASK-64.1 changed make_move_unchecked to take &mov and gave quiesce an explicit ply arg).
1. Merge current primary (master) into the task branch.
2. Resolve the engine/src/search.rs conflict: place both prefetch(self.pos.zobrist().0) hints immediately after the new make_move_unchecked(&mov) in the main search and quiescence, before the ply-carrying quiesce recursion. Reconcile any benches/search.rs and Search::trace() drift too.
3. Keep the tt.rs prefetch method, the prefetch/layout tests, the hash-load benchmark, and BENCHMARKS.md unchanged (additive, no defect found in review).
4. Re-run required checks (fmt, clippy -D warnings, test --workspace) and the hash-load benchmark smoke.
5. Hand off a fresh immutable target for a new independent review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Evaluated two TT hot-path candidates against a new hash-loading benchmark; retained the prefetch, rejected storing the static eval. Full measurement narrative is in BENCHMARKS.md under 'Transposition-table hot-path enhancements'.

### Benchmark harness (AC#1)
- benches/search.rs gains a 'search hash load' group over four positions (startpos d9, kiwipete d8, middlegame d8, endgame d11) at fixed depths that load a 16MB table to 51-100% occupancy. Table is cleared outside the timed region so every iteration searches the whole tree; the pre-existing depth-7 pair cannot see a TT change because criterion re-runs it against a warm table (135k nodes collapse to 579).
- Exact, run-to-run-reproducible baseline printed before timings: startpos 2,501,994 nodes / 45.6% hit / hashfull 648; kiwipete 5,241,036 / 20.6% / 1000; middlegame 5,780,828 / 21.3% / 1000; endgame 1,839,611 / 48.2% / 513. Per-node cost ~75-82 ns.
- Added Search::trace() to expose the tracer for telemetry.

### Static eval, rejected (AC#2, AC#4)
- New 'static evaluation' microbench: material_eval is 2.8 ns, i.e. 3.6% of a ~78 ns node, and that is an unreachable ceiling (must compute once to store; only 20-48% of probes hit). Rejected on cost.
- Also rejected on entry space: the data word has exactly 15 spare reserved bits (the entry's only migration headroom); an i16 eval needs 16, so it would widen the 16-byte slot and halve density again on top of TASK-57.
- TASK-50/51/52 interaction: futility and null-move pruning read the eval of the node they are already at (search step 6), not an ancestor's or a stored one, so a table-resident eval buys the imminent pruning consumers nothing.
- Revisit condition recorded: a non-material-only evaluation (PSQT/NNUE, tens-hundreds of ns) changes the arithmetic.
- Interaction with TASK-58 rule-sensitive policy: not applicable, because the candidate was not implemented. A stored eval would have been position-intrinsic (evaluate() does not read the clock), consistent with TASK-58 rule 3, but no such field exists.

### Prefetch, retained (AC#3)
- Table::prefetch: _mm_prefetch (x86_64), inline 'prfm pldl1keep' (aarch64, since core::arch::aarch64::_prefetch is unstable), empty body elsewhere. Called after make_move in both main search and quiescence, at the earliest point the child key exists.
- Retained on mechanism/risk, not a measured figure: node counts identical by construction (a hint changes no visible state), the prefetched cluster is exactly what the child probes, and the mechanism is standard. A clean speedup was unobtainable: every round ran under sustained concurrent load (load avg 4-6 from other worktrees' benchmarks), which is the worst case for a latency-hiding benchmark. Minimum-of-6 was startpos -5.9%, endgame +0.8% (non-negative, not repeatable); documented as inconclusive, not cited as the effect.
- Decision to keep on mechanism grounds was confirmed with the user this session.
- Cost: one unsafe hint per architecture. prefetch_moves_no_observable_state pins that the hint perturbs nothing a probe returns and is total over keys.

### Entry layout (AC#5)
- Layout is unchanged (only a method was added), so the existing cluster_is_one_cache_line_and_slots_fill_it test still asserts the final layout: Cluster 64 bytes / align 64, Slot 16 bytes, 4 slots per cache line. clusters_are_cache_line_aligned_in_the_allocation covers alignment in the allocation.

## Re-integration (stale-base rework)

The approved target b76a0c2 could not be merged: after its base c55508b, TASK-64.1 (explicit-ply search stack) landed on primary, changing make_move_unchecked to take &mov and giving quiesce an explicit ply argument, which conflicted with the two prefetch insertions. No review defect; a stale-base integration conflict.

Resolved by merging current primary (aa915d8) into the task branch. The only conflict was the quiescence hunk in engine/src/search.rs; resolved by keeping primary's new signatures and placing self.tt.prefetch(self.pos.zobrist().0) between the make_move_unchecked(&mov) and the ply-carrying quiesce recursion. The main-search prefetch hunk auto-merged onto the new &mov signature. tt.rs (prefetch method + test), benches/search.rs, and BENCHMARKS.md merged without conflict and are unchanged from the approved target.

New immutable target: c6bf762 (base aa915d8). Code diff base..target is exactly the four task files (BENCHMARKS.md, benches/search.rs, engine/src/search.rs +20, engine/src/tt.rs +70).

Verification after re-integration:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: pass (43 + 265 + 19 + 1; 2 ignored pre-existing perft suites), incl. prefetch_moves_no_observable_state and cluster_is_one_cache_line_and_slots_fill_it
- cargo bench --bench search 'hash load/startpos': runs against the new APIs; baseline telemetry reproduces exactly (startpos 2,501,994 nodes / 45.6% hit / hashfull 648), confirming the re-integrated prefetch stays node-count-neutral. Criterion's reported timing 'change' is against a stale saved sample under concurrent machine load, not a node change.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 19:55
---
Implementation handoff
Branch: task-61-tt-hot-path-enhancements
Worktree: /Users/seabo/seaborg-worktrees/task-61-tt-hot-path-enhancements
Base: c55508b3383577ed9bb62a9ebadb21fc3ecedc1f
Implementation target: b76a0c234169623d7e5d519b1f34bc7c052fb74c
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (43 + 245 + 17 + 1 passed; 2 ignored are the pre-existing long perft suites)
- cargo bench --bench search -- "hash load": runs; baseline table reproduces
Known failures: none

Reviewer note: AC#3's repeatable-benefit measurement could not be obtained on this machine; it carried sustained load (avg 4-6) from concurrent worktree benchmarks for the whole session, and a prefetch benchmark is the worst case for that. The prefetch is retained on mechanism and risk (node-count-neutral by construction, hint never wasted, standard technique, contained unsafe cost), a call confirmed with the user this session. The inconclusive figures and full rationale are in BENCHMARKS.md. If a genuinely idle machine is available, a clean round-robin of 'search hash load' base vs target would let the decision be promoted from mechanism-based to measurement-based.
---

author: @claude
created: 2026-07-19 20:08
---
Review attempt: 1
Reviewed branch: task-61-tt-hot-path-enhancements
Reviewed implementation: b76a0c234169623d7e5d519b1f34bc7c052fb74c
Verdict: approved

Independent review of the full base-to-target diff (c55508b..b76a0c2). Target immutability confirmed: target descends from base, the sole post-target commit (954bd82) touches only the task file, and the code diff target..tip is empty.

Acceptance criteria:
- AC#1 (baseline harness): PROVEN. benches/search.rs 'search hash load' group over four fixed-depth positions with the table cleared outside the timed region, plus report_hash_load_telemetry printing exact nodes/probes/hits/hashfull. Documented in BENCHMARKS.md.
- AC#2 (static eval specified + gated): PROVEN. Value/validity conditions specified; rejected on measured 2.8 ns material_eval (3.6% ceiling of a ~78 ns node, unreachable given 20-48% hit rates) and on entry space (15 spare reserved bits vs 16 for an i16); TASK-58 interaction addressed (position-intrinsic); revisit condition recorded.
- AC#3 (prefetch evaluated + conditionally retained): ACCEPTED on mechanism grounds. No repeatable speedup could be measured; the machine carried sustained load (avg ~6, worst case for a latency-hiding benchmark) and a clean base-vs-target round-robin was unobtainable. The reviewer independently confirmed the same load condition. Retention is justified by proven node-count neutrality (a hint changes no observable state; pinned by prefetch_moves_no_observable_state) and proven portability/safety no-harm (empty body on unsupported targets; contained, justified unsafe per architecture). The task owner accepted mechanism-based retention in lieu of a measured benefit during this review session. The benefit remains explicitly unquantified; BENCHMARKS.md records the revisit-on-idle-hardware condition.
- AC#4 (coverage + recorded rejection): PROVEN. Rejected candidate's measurements and decision are in BENCHMARKS.md; retained candidate has the node-neutrality regression test and the benchmark harness.
- AC#5 (compact layout asserted): PROVEN. Layout unchanged; cluster_is_one_cache_line_and_slots_fill_it and clusters_are_cache_line_aligned_in_the_allocation pass.

Comment quality, SAFETY justifications, and #[allow] usage checked: comments are self-contained; no #[allow] introduced; no scope creep.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (fresh CARGO_TARGET_DIR)
- cargo test --workspace: pass (43 + 245 + 17 + 1; 2 ignored pre-existing perft suites)
- Hot-path benchmark round-robin: not run to a verdict; sustained machine load (avg ~6.1) precludes a clean measurement, and the change is node-count-neutral by construction so it carries no search-quality regression risk.
---

author: @claude
created: 2026-07-19 20:12
---
Merge attempt: 1
Verdict: ejected — textual conflict on integration
Primary tip tested: aa915d85d32d03d829d0636c6af3e71b40a6632f
Merged tip: 641b7f5 (approved code target b76a0c2)

git merge --no-ff of the approved target onto the live primary tip conflicts in engine/src/search.rs. Since the approved target's base (c55508b), TASK-64.1 (explicit-ply search stack) landed on primary and changed two call sites this task also edits:
- make_move_unchecked now takes a reference: `self.pos.make_move_unchecked(&mov)` (was by value).
- quiesce now takes an explicit ply: `self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound(), ply + 1)` (previously no ply argument).

The task inserts `self.tt.prefetch(self.pos.zobrist().0)` after make_move in both the main search and quiescence, against the old signatures. The main-search hunk auto-merges; the quiescence hunk is a true content conflict (both sides changed the make_move line and the quiesce call). Re-integrating the prefetch against the new signatures is implementation work and is out of scope for the merge gate.

Rework: reattach this branch's worktree, merge current primary (or rebase the two prefetch insertions onto it), place both prefetch calls after the new `make_move_unchecked(&mov)` and before the ply-carrying quiesce call, re-run the required checks, and hand off a new immutable target for a fresh review. No implementation defect was found in review; this is purely a stale-base integration conflict.

Verification:
- git checkout --detach aa915d8 && git merge --no-ff 641b7f5: CONFLICT (content) in engine/src/search.rs; merge aborted, primary not advanced.
---

author: @claude
created: 2026-07-19 20:18
---
Implementation handoff
Branch: task-61-tt-hot-path-enhancements
Worktree: /Users/seabo/seaborg-worktrees/task-61-tt-hot-path-enhancements
Base: aa915d85d32d03d829d0636c6af3e71b40a6632f
Implementation target: c6bf7622fd657932389b59b68fe9940edde898d1
Resolved findings: none (stale-base re-integration of previously-approved b76a0c2; no review defect was raised)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: pass (43 + 265 + 19 + 1; 2 ignored pre-existing perft suites)
- cargo bench --bench search -- 'hash load': runs against current APIs; baseline telemetry reproduces exactly (node counts unchanged), confirming prefetch node-neutrality after re-integration
Known failures: none

Reviewer note: this is a fresh immutable target created solely to re-integrate the approved b76a0c2 work onto current primary after a merge eject. The code delta from that prior-approved target is limited to the two prefetch call sites now sitting after make_move_unchecked(&mov) and before the ply-carrying quiesce recursion (TASK-64.1 signatures); tt.rs, benches/search.rs and BENCHMARKS.md are unchanged from the approved target. AC#3's benefit remains mechanism-based and explicitly unquantified as before (documented in BENCHMARKS.md); the machine still carries concurrent load, so a clean round-robin was not obtainable.
---

author: @claude
created: 2026-07-19 20:26
---
Review attempt: 2
Reviewed branch: task-61-tt-hot-path-enhancements
Reviewed implementation: c6bf7622fd657932389b59b68fe9940edde898d1
Verdict: approved

Independent review of the full base-to-target diff (aa915d8..c6bf762). This target is a stale-base re-integration of the previously-approved b76a0c2 onto current primary after a merge eject (comment #3). Target immutability confirmed: c6bf762 descends from base aa915d8, the sole post-target commit (652787a) touches only the task file, and the code diff target..tip is empty.

Re-integration faithfulness: tt.rs, benches/search.rs and BENCHMARKS.md are byte-identical to the approved b76a0c2 (empty diff). The search.rs task-delta over the new primary is exactly the two prefetch calls, correctly placed against TASK-64.1's new signatures: self.tt.prefetch(self.pos.zobrist().0) sits after 'unsafe { self.pos.make_move_unchecked(&mov) }' in both the main search and quiescence, and before the ply-carrying quiesce(..., ply + 1) recursion. The aa915d8..c6bf762 code diff is exactly Search::trace() + 2 prefetch calls (search.rs), Table::prefetch + prefetch_moves_no_observable_state (tt.rs), the 'search hash load' / 'static evaluation' bench harness (benches/search.rs), and BENCHMARKS.md. No accidental or out-of-scope changes.

Acceptance criteria:
- AC#1 (baseline harness): PROVEN. benches/search.rs 'search hash load' group over four fixed-depth positions with the table cleared outside the timed region, plus report_hash_load_telemetry printing exact nodes/probes/hits/hashfull. Documented in BENCHMARKS.md.
- AC#2 (static eval specified + gated): PROVEN. Value/validity conditions specified; rejected on measured 2.8 ns material_eval (3.6% ceiling of a ~78 ns node, unreachable given 20-48% hit rates) and on entry space (RESERVED_MASK = 0x7FFF<<48 is exactly 15 spare bits vs 16 for an i16); TASK-50/51/52 read the current node's eval so gain nothing; TASK-58 interaction addressed (position-intrinsic); revisit condition recorded.
- AC#3 (prefetch evaluated + conditionally retained): ACCEPTED on mechanism grounds. The positive speed benefit is unquantified: the machine is under sustained load (I independently confirmed load avg ~11), the worst case for a latency-hiding benchmark, so a clean base-vs-target round-robin is unobtainable, exactly as documented. Retention is justified by affirmatively-proven no-harm: node-count neutrality by construction (a hint changes no observable state; pinned by prefetch_moves_no_observable_state), portability (empty body on unsupported targets), and safety (two contained unsafe blocks each with a correct SAFETY comment; x86_64 _mm_prefetch cannot fault, aarch64 prfm hint is readonly/nostack/preserves_flags). The task owner accepted mechanism-based retention in lieu of a measured benefit; BENCHMARKS.md records the revisit-on-idle-hardware condition. This matches review attempt 1's finding on the identical code.
- AC#4 (coverage + recorded rejection): PROVEN. Rejected candidate's measurements and decision are in BENCHMARKS.md; retained candidate has the node-neutrality regression test and the benchmark harness.
- AC#5 (compact layout asserted): PROVEN. Layout unchanged; cluster_is_one_cache_line_and_slots_fill_it and clusters_are_cache_line_aligned_in_the_allocation pass.

Comment quality, SAFETY justifications, and #[allow] usage checked: comments are self-contained; no #[allow] introduced; no scope creep.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean (fresh CARGO_TARGET_DIR)
- cargo test --workspace: pass (43 + 265 + 19 + 1; 2 ignored pre-existing perft suites)
- Hot-path benchmark round-robin: not run to a verdict; sustained machine load (avg ~11) precludes a clean measurement, and the change is node-count-neutral by construction so it carries no search-quality regression risk.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Evaluated two TT hot-path candidates against a new reproducible hash-loading search benchmark: retained child-cluster prefetching, rejected storing the static eval. Reviewed re-integration target c6bf762 (base aa915d8), a faithful re-integration of the previously-approved b76a0c2 onto current primary after a stale-base merge eject; tt.rs, benches/search.rs and BENCHMARKS.md are byte-identical to the approved target, and the two prefetch calls sit correctly after make_move_unchecked(&mov) and before the ply-carrying quiesce (TASK-64.1 signatures). Verified on the target: cargo fmt --check (pass); cargo clippy --workspace --all-targets --all-features -- -D warnings (clean, fresh CARGO_TARGET_DIR); cargo test --workspace (43+265+19+1 pass, 2 pre-existing ignored perft suites), incl. prefetch_moves_no_observable_state and cluster_is_one_cache_line_and_slots_fill_it. AC#3's positive speed benefit is unquantified: the machine remains under sustained load (avg ~11), independently confirmed, so a clean round-robin is unobtainable; prefetch retained on mechanism grounds (proven node-count-neutral and portability/safety no-harm, standard technique) per the task owner's recorded decision, with the idle-hardware revisit condition recorded in BENCHMARKS.md.
<!-- SECTION:FINAL_SUMMARY:END -->
