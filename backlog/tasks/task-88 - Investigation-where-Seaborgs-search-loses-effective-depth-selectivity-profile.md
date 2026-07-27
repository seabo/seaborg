---
id: TASK-88
title: >-
  Investigation: where Seaborg's search loses effective depth (selectivity
  profile)
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-27 10:15'
updated_date: '2026-07-27 12:50'
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
- [x] #1 Seaborg's own selectivity profile is measured on a representative suite at fixed time and fixed nodes and recorded (EBF, depth reached, first-move-cutoff rate, LMR re-search rate and reduction distribution, quiescence fraction, PV/non-PV and aspiration re-search rates), using only Seaborg's instrumentation
- [x] #2 The findings identify from first principles where effective depth is lost, each backed by our own measured signal rather than a comparison to another engine
- [x] #3 A ranked list of candidate selectivity experiments is produced, each individually testable by self-play SPRT at a real time control, with an expected mechanism stated
- [x] #4 Any external-engine comparison is limited to a single lightweight sanity check and is explicitly not the basis for any recommendation
- [x] #5 Methodology (suite, time and node budgets, instrumentation, hardware) is recorded for reproducibility, and no permanent engine behaviour change is shipped by this task
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add an off-by-default 'selstats' cargo feature (engine + root binary). Default build stays byte-identical; instrumentation and its hot-loop cost exist only under the feature, so no permanent engine behaviour change ships.
2. Under the feature, extend Tracer (engine/src/trace.rs) with a SelStats counter block and feature-gated increment methods; wire call sites in search.rs at the exact selectivity points: node-type entry (pv/nonpv), beta-cutoff move index (first-move-cutoff + histogram + fail-high by type), node result classification (exact/fail-low by type), LMR applied/re-search + reduction-ply distribution, PV scout + PVS re-search, aspiration windowed iterations + fail-high/fail-low, non-PV TT early cutoffs, and qsearch in-check widening. Emit a compact JSON summary as a feature-gated SearchEvent::SelStats -> 'info string selstats {json}' at end of iterative deepening (nodes, qnodes, q-fraction, EBF, hash hit/miss/collision, TT-move availability, killer slots, all sel counters).
3. Add tools/diag/selectivity_profile.py (reuse uci.py driver + bench-positions.epd): run the 20-position suite at fixed depth and fixed nodes with --features selstats, parse the selstats line per position, aggregate node-weighted ratios, emit JSON + a markdown summary table. Use the uninstrumented release for the fixed-time depth/EBF reality check (instrumentation perturbs wall-clock but not ratios/nodes).
4. Measure on the local host (Apple M3 Pro), record methodology (suite, depth/node/time budgets, instrumentation, hardware). One lightweight external SF depth-at-time sanity check only (cite TASK-82's already-recorded reading), explicitly not the basis of any recommendation.
5. Write docs/selectivity-profile.md: Seaborg's own selectivity profile, first-principles diagnosis of where effective depth is lost (each backed by our measured signal), and a ranked list of candidate self-play-SPRT experiments with expected mechanisms. Ship no permanent engine behaviour change.
6. Required checks: cargo fmt --check; cargo clippy --workspace --all-targets --all-features -D warnings (this compiles the gated code); cargo test --workspace. Hand off to review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Approach: added an off-by-default 'selstats' cargo feature (engine + root binary). With it off (every shipped build) the SelStats counters, their hot-loop increment sites, and the JSON emission compile out entirely, so engine behaviour and speed are unchanged - verified byte-for-byte on the tree: a depth-15 search of the kiwipete-family position visits 1,204,096 nodes under both the default and the selstats release. This is why AC#5's 'no permanent engine behaviour change' holds: the instrumentation is temporary/opt-in, not a behaviour change to the search.

Instrumentation (engine/src/trace.rs::SelStats, wired in search.rs): node-type entry (pv/non-pv), beta-cutoff move index (first-move-cutoff + histogram + fail-high by type), completed-loop outcome (exact/fail-low by type), LMR applied + re-search + reduction-ply distribution, PV scout + PVS re-search, aspiration windows + fail-high/fail-low, non-PV TT early cutoffs, quiescence in-check widening; plus the always-on node/TT/killer figures. Emitted once per search as a feature-gated SearchEvent::SelStats -> 'info string selstats {json}'.

Driver: tools/diag/selectivity_profile.py (reuses tools/diag/uci.py + bench-positions.epd), pooling each rate's numerator/denominator across the 20-position suite.

Measured profile (Apple M3 Pro, native release, 64MB hash, single thread; fixed depth 14 + fixed 2M nodes instrumented, fixed 2000ms depth from the uninstrumented release):
- Depth at 2000ms: mean 15.3 / median 15; EBF 2.55-2.86.
- LMR re-search rate 1.6-2.0% (mean reduction ~2.1 ply, tail >=4 ply <7%) -> reductions almost never overturned = under-reduction, the clearest depth-loss signal.
- Quiescence 48-54% of all nodes (~16% of qnodes in check).
- TT-move availability 29-37% (only ~1/3 of nodes ordered with a hash move).
- First-move-cutoff 88.8% (ordering strong, not a loss site); PVS re-search 5.1%; aspiration re-search 12-13%/window.

Report + ranked experiments + methodology written to BENCHMARKS.md ('Search selectivity profile (self-instrumented)'). External comparison limited to citing TASK-82's single EBF/depth reading as a sanity check, explicitly not a basis for any recommendation. No follow-up Backlog tasks created (deferred to the human per the philosophy that the scope decision is theirs).

Follow-up (human-directed, same branch): the pooled TT-move-availability finding was the weakest in the report, so added a node-type x TT-hit/miss split to the instrumentation and re-measured. Result rules TT availability out as a depth-loss site: PV nodes 79-97% covered; the low pooled ~30% is the first-visit non-PV frontier that no engine has a stored move for; and the ordering penalty of a missing TT move is only ~5-7 pp (first-move-cutoff 86-87% at TT-miss vs 91-93% at TT-hit). Non-TT ordering (capture-history/SEE, killers, counter-move, quiet history) is strong enough that the hash move is a refinement, not load-bearing; consistent with IIR's +28 Elo coming from reducing TT-miss nodes, not needing better ordering there. BENCHMARKS.md updated: finding rewritten as 'investigated and ruled out', TT-availability experiment removed from the ranked list. Two strong findings unchanged (LMR under-reduction; quiescence ~half the tree). Behaviour still transparent (identical 1,204,096-node depth-15 search).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-27 10:50
---
Implementation handoff
Branch: task-88-selectivity-profile
Worktree: /Users/seabo/seaborg-worktrees/task-88-selectivity-profile
Base: 4722f814d5e1be00d4526dd06993ff4ff48d48d5
Implementation target: 1a287cf51b74a77206a7d9b5af46bcdd2d16c809
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean; --all-features compiles the gated selstats code)
- cargo test --workspace: pass (clean idle run: 441 engine + 157 lichess + workspace suites, 0 failed, 2 ignored). engine also green with --features selstats (441 passed). Behaviour transparency verified: depth-15 kiwipete-family search visits 1,204,096 nodes under both the default and selstats release.
Known failures: none. On one earlier full-workspace run under concurrent build+test load, the lichess timing test run::tests::incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked flaked once with 'matchmaking issued an outgoing challenge: Timeout'; it passes 3/3 in isolation and the lichess crate is untouched by this diff (selstats does not forward to lichess), and the subsequent idle full-workspace run was clean.
Notes for reviewer: this is a spike. The engine change is the off-by-default 'selstats' instrumentation feature only; the default/shipped build is behaviour- and speed-identical (feature off => counters, hot-loop increments, and emission compile out). Deliverables: the selectivity profile + first-principles diagnosis + ranked SPRT experiment list in BENCHMARKS.md ('Search selectivity profile (self-instrumented)'), and tools/diag/selectivity_profile.py. Measured on Apple M3 Pro (scalar-NNUE ARM release); ratios/node counts are ISA-independent, the 2000ms depth figure is host-dependent. No follow-up Backlog tasks were created for the ranked experiments (the scope decision is the human's).
---

author: @claude
created: 2026-07-27 12:35
---
Implementation handoff (updated target)
Branch: task-88-selectivity-profile
Worktree: /Users/seabo/seaborg-worktrees/task-88-selectivity-profile
Base: 4722f814d5e1be00d4526dd06993ff4ff48d48d5
Implementation target: f144f8915d7964053ab7db7e8705d6260989d787
Supersedes prior target 1a287cf51b74a77206a7d9b5af46bcdd2d16c809 (human-directed follow-up added a node-type / TT-hit-miss split; review the new tip).
Resolved findings: none (no review findings; this is a scope extension)
Verification (clean idle run):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean; --all-features compiles the gated selstats code including the new split)
- cargo test --workspace: pass (441 engine + 157 lichess + workspace suites, 0 failed, 2 ignored); engine also green with --features selstats (441 passed). Behaviour transparency re-verified: depth-15 kiwipete-family search visits 1,204,096 nodes under both default and selstats builds after the change.
Known failures: none.
Change since prior target: engine/src/trace.rs + search.rs add node-type x TT-availability and TT-hit/miss cutoff counters; tools/diag/selectivity_profile.py aggregates/derives the split; BENCHMARKS.md rewrites the TT-move-availability finding as 'investigated and ruled out' and removes that experiment from the ranked list. No default-build behaviour change (selstats off => compiles out).
---

author: @claude
created: 2026-07-27 12:50
---
Review attempt: 1
Reviewed branch: task-88-selectivity-profile
Reviewed implementation: f144f8915d7964053ab7db7e8705d6260989d787
Verdict: approved

All five acceptance criteria proven by objective evidence; no blocking findings.

AC#1 (profile measured/recorded, Seaborg-only instrumentation): met. BENCHMARKS.md 'Search selectivity profile (self-instrumented)' records EBF, depth-at-time, first-move-cutoff rate, LMR re-search rate + reduction distribution, quiescence fraction, PV/non-PV and aspiration re-search rates, at fixed depth 14 and fixed 2M nodes (instrumented) plus depth-at-2000ms from the uninstrumented release. Reproduced live via tools/diag/selectivity_profile.py on the bench suite (first-move-cutoff 88.8%, LMR re-search 1.68%, mean reduction 2.10 ply, quiescence ~55%).
AC#2 (first-principles findings, each backed by our own signal): met. LMR under-reduction (1.6-2.0% re-search) and quiescence ~half the tree, each tied to a measured counter; no cross-engine behaviour diffing.
AC#3 (ranked SPRT-testable experiments with mechanism): met. Five ranked experiments at 10s+0.1s, each with an expected mechanism, signal to watch, and named tunables (LMR_BASE/LMR_DIVISOR/LMR_MOVE_THRESHOLD/QUIESCENCE_DELTA_MARGIN/ASPIRATION_INITIAL_DELTA — all confirmed present in engine/src/search.rs).
AC#4 (single external sanity check, not a basis): met. Only TASK-82's single EBF/depth reading is cited, explicitly not a basis for any recommendation.
AC#5 (methodology recorded; no permanent behaviour change): met. Suite/budgets/hardware/driver and reproduce commands recorded. Instrumentation is off-by-default and compiles out; every counter is written after the decision it observes.

Verification (on the implementation target, this machine):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean CARGO_TARGET_DIR=/tmp/task88-clippy, so the gated selstats code was actually linted)
- cargo clippy --workspace --all-targets -- -D warnings (default features): pass
- cargo test --workspace: pass (0 failed; 10 'test result: ok' summaries, doctests included)
- Behaviour transparency: default and selstats release builds both visit 1,017,413 nodes and return bestmove e2e4 on startpos depth 15 (identical tree). Perft/movegen benchmarks not run: every added statement in the shipped hot path is #[cfg(feature="selstats")]-gated and compiles out, so the default machine code is unchanged.
- Deliverable end-to-end: tools/diag/selectivity_profile.py drives a --features selstats build over bench-positions.epd and emits the reported profile.

Scope: base 4722f814 -> target f144f891 touches only in-scope files (selstats feature + instrumentation + driver + BENCHMARKS.md + necessary SearchEvent match arms). Commit 203030d after the target touches only the task file (metadata). Worktree clean.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds an off-by-default 'selstats' cargo feature that instruments Seaborg's own search (engine/src/trace.rs::SelStats wired at the selectivity decision points in search.rs, emitted once per search as 'info string selstats {json}'), a stdlib driver tools/diag/selectivity_profile.py that pools the counters across the 20-position bench suite, and a self-instrumented selectivity report + ranked first-principles experiment list in BENCHMARKS.md. Findings: LMR is under-reducing (re-search rate 1.6-2.0%, mean ~2.1 ply) and quiescence is ~half the tree; TT-move availability investigated and ruled out via a node-type x TT-hit/miss split. Every counter is written after the decision it observes, so shipped (feature-off) behaviour is unchanged. Verified: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -D warnings pass on a clean CARGO_TARGET_DIR (compiles the gated code) and default-feature clippy pass; cargo test --workspace pass (0 failed); behaviour transparency confirmed independently — default and selstats release builds both visit 1,017,413 nodes with identical bestmove e2e4 on startpos depth 15; the driver reproduces the reported profile on the bench suite.
<!-- SECTION:FINAL_SUMMARY:END -->
