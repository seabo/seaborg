---
id: TASK-97.7
title: 'A1: Oracle-ordering ceiling (the single most important measurement)'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-29 18:46'
updated_date: '2026-07-29 20:28'
labels:
  - search
  - selectivity
  - investigation
dependencies:
  - TASK-97.1
references:
  - engine/src/search.rs
  - engine/src/ordering.rs
  - engine/src/pv_table.rs
parent_task_id: TASK-97
priority: high
type: spike
ordinal: 172000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track A / item A1 — the single most important measurement in the investigation. Hypothesis: a large fraction of our EBF gap is pure ordering waste, recoverable with zero soundness cost; if we seed every node with its TRUE best move first, EBF collapses toward the minimal-tree floor (sqrt b). Method: a two-pass instrumented build. Pass 1 does a full search and records the best move per position/node (or harvests from a deep TT). Pass 2 re-searches the same fixed depth with those moves forced to the front of ordering and counts nodes. Compare EBF real → oracle at fixed depth 12–16 on bench-positions.epd. Metrics: EBF_real, EBF_oracle, EBF_frontier (~2.01). Decision: EBF_real − EBF_oracle is the ceiling on what ordering ALONE can buy (free — do it first); EBF_oracle − 2.01 is the part that is genuinely pruning/eval (flywheel-gated — defer). This draws the line the whole strategy hinges on. Needs a new build; highest value. Depends on A2 (TASK-97.1) which scopes it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A two-pass oracle-ordering build measures node counts at fixed depth (12–16) on bench-positions.epd with true best moves forced to the front of ordering at every node
- [ ] #2 EBF_real, EBF_oracle, and the frontier reference (~2.01) are reported, and the split EBF_real − EBF_oracle (free ordering headroom) versus EBF_oracle − frontier (eval/pruning-limited) is stated
- [ ] #3 The instrumentation is temporary; no permanent engine change ships
- [ ] #4 The result explicitly feeds guardrail 4: whether the free-EBF ordering lever is large enough to pursue
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add off-by-default cargo feature `oracle` (like `selstats`): shipped default build compiles it out entirely, so no permanent engine behaviour/speed change ships (satisfies AC#3 the same way selstats does).

2. Engine hooks, all #[cfg(feature="oracle")], measuring ORDERING ALONE (leave tt_mov, IIR, singular, pruning exactly as the real search computes them):
   - Search gains 3 gated fields: oracle_active: bool, oracle_force: HashMap<u64,Move> (moves to force first; empty ⇒ real pass), oracle_record: HashMap<u64,(Move,Depth)> (best move per position recorded this pass, depth-preferred).
   - Pass-1 recording: at the Step-24 TT store site (search.rs ~2991-3013, inside the existing history-draw/root-exclusion guard), when best_move is non-null, record (zobrist -> (best_move, remaining depth)), keeping the deeper entry.
   - Ordering injection: in MoveLoader::load_hash, when oracle_active and oracle_force has a valid_move() entry for pos.zobrist().0, push that move in the hash phase instead of the TT move. Only move ordering changes; the real tt_mov still flows to IIR/singular unchanged, and appears in its normal generated phase (deduped once). No legal move dropped.

3. Two-pass driver: pub fn SearchEngine::oracle_profile(&mut self, pos, depth, iterations) -> Vec<OraclePass{main_nodes, all_nodes, ebf, depth_reached}> (gated). For pass 0..=iterations: clear_hash (cold TT each pass — mandatory so EBF_oracle is not inflated by a warm TT); build a single-thread Search with the built-in net (self.network) via Search::build; set oracle_active=true and oracle_force=<prev pass recorded map> (empty for pass 0 = real); run::<Worker>(depth); read EBF/nodes from trace; take oracle_record as next pass's force map. Pass 0 = EBF_real, pass 1 = EBF_oracle (the specified two-pass). Extra iterations show convergence.

4. Harness: engine/examples/oracle_ordering.rs with [[example]] required-features=["oracle"] in engine/Cargo.toml (matches ordering_ablation.rs). Reads bench-positions.epd, runs oracle_profile at fixed depth (default 14; also 12 and 16) with the default 64 MB hash, prints per-position and aggregate EBF_real, EBF_oracle, frontier ~2.01, and the split EBF_real-EBF_oracle (free ordering headroom) vs EBF_oracle-frontier (eval/pruning-limited). Geomean across suite (matches the re-baseline geomean convention).

5. Run at depth 12/14/16 on bench-positions.epd; record EBF_real, EBF_oracle, split, and the guardrail-4 read (is the free-ordering lever large?) in task notes and a BENCHMARKS.md Phase-4/A1 section (documentation only). Verify EBF_real reproduces the recorded baseline (2.71 @ d14) as a sanity check.

6. cargo fmt --check, clippy -D warnings (default AND --features oracle), cargo test --workspace. AC#3: default build unchanged; oracle path only exists under the feature and in an example gated by required-features.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
A1 oracle-ordering ceiling. Added an off-by-default `oracle` cargo feature (mirrors `selstats`: the shipped default build compiles the oracle fields, the per-node recording and the ordering injection out entirely, so behaviour/speed are unchanged). No permanent engine change ships (AC#3).

Method (two-pass, ordering-only): #[cfg(feature="oracle")] hooks on Search — pass 1 records the best move proved at each completed node at the Step-24 TT-store site (keyed by Zobrist, deepest-proof wins); pass 2 forces that move to the front in MoveLoader::load_hash (legality-guarded against Zobrist collisions). Only ordering changes: the resolved TT move still drives IIR and the singular check and is searched in its own generated phase, so no legal move is dropped. SearchEngine::oracle_profile runs the passes from a cold table each time (mandatory — a warm table would supply the ordering itself); a new engine/examples/oracle_ordering.rs (required-features=["oracle"]) drives bench-positions.epd through the built-in network and reports the split.

Results (geomean over 20 positions, 64 MB hash, built-in net):
- depth 12: EBF_real 2.935, EBF_oracle 2.895; headroom 0.040 (4.4% of frontier gap); node ratio 0.864.
- depth 14: EBF_real 2.695 (arith mean 2.710 = recorded baseline 2.71, validated), EBF_oracle 2.649 (1 pass) / 2.628 (converged, 2 passes); headroom 0.046 (6.8%); node ratio 0.806.
- depth 16: EBF_real 2.514, EBF_oracle 2.470; headroom 0.045 (8.9%); node ratio 0.773.
Frontier reference 2.01; eval/pruning-limited remainder (oracle-frontier) 0.885/0.639/0.460.

AC#1: two-pass build measures node counts at fixed depth 12/14/16 on bench-positions.epd with true best moves forced to the front at every node. AC#2: EBF_real, EBF_oracle and frontier ~2.01 reported; split real-oracle (free ordering, 0.04-0.05) vs oracle-frontier (eval/pruning-limited, 0.46-0.89) stated. AC#3: temporary; behind the oracle feature + a required-features example, default build unchanged. AC#4: feeds guardrail 4 — the free-EBF ordering lever is small (4-9% of the frontier gap); the gap is overwhelmingly eval/pruning-limited (flywheel-gated).

Honest caveats (in BENCHMARKS.md Phase 4 and the example docs): the effect is bimodal (tactical cut-node-heavy positions ~halve; quiet all-node-heavy positions worsen) and node savings grow with depth (ratio 0.86->0.77). Forcing best-first at full depth interacts with LMR, so EBF_oracle is a conservative 'best-first under the engine's real reductions' figure, not a strict lower bound — true headroom is >= measured, but even generously a minority of the frontier gap. Findings documented in BENCHMARKS.md Phase 4 (documentation, not engine behaviour).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-29 19:44
---
Implementation handoff
Branch: task-97.7-oracle-ordering-ceiling
Worktree: /Users/seabo/seaborg-worktrees/task-97.7-oracle-ordering-ceiling
Base: a360df285502e85d85d4a8c9a0f5be88cc54a5ee
Implementation target: 2594608a7a4cafc919457a7411f2fc2dbe6996e5
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (no warnings; --all-features exercises the oracle path and the oracle_ordering example)
- cargo test --workspace: PASS (all suites green; default features, oracle path compiled out)
- Reproducibility: EBF_real arithmetic mean 2.710 at depth 14 reproduces the recorded fixed-depth-14 baseline (2.71) exactly
Known failures: none

Scope note for reviewer: spike / measurement only. base..target diff touches engine/Cargo.toml (new off-by-default `oracle` feature + required-features example entry), engine/src/search.rs (all new code under #[cfg(feature="oracle")]), engine/examples/oracle_ordering.rs (required-features=["oracle"]), and BENCHMARKS.md (Phase 4 write-up, documentation). AC#3 (no permanent engine change) satisfied by construction: default build is byte-behaviour unchanged, same standard as the existing selstats feature. To reproduce: RUSTFLAGS="-C target-cpu=native" cargo run --release -p engine --features oracle --example oracle_ordering -- tools/diag/bench-positions.epd 14 2 64 (and depths 12, 16).
---

author: @claude
created: 2026-07-29 20:04
---
Review attempt: 1
Reviewed branch: task-97.7-oracle-ordering-ceiling
Reviewed implementation: 2594608a7a4cafc919457a7411f2fc2dbe6996e5
Verdict: changes_requested

Scope is exactly as handed off (engine/Cargo.toml oracle feature + required-features example, all search.rs additions #[cfg(feature="oracle")], engine/examples/oracle_ordering.rs, BENCHMARKS.md Phase 4). The measurement is sound and reproducible: I re-ran depth 12 and it matches BENCHMARKS.md exactly (EBF_real 2.935, EBF_oracle 2.895, headroom 0.040, node ratio 0.864, 4.4% of gap). Ordering-only isolation is correct — load_hash injects the oracle move into the hash segment, and OrderedMoves::segregate_duplicates de-dups later phases against that segment (not self.hash_move), so the real TT move still flows through its generated phase (no move dropped, no double search) and still drives IIR/singular; the valid_move guard covers Zobrist collisions. Two blocking documentation/harness defects remain in the deliverable.

REV-1-01 [P2] BENCHMARKS.md Phase 4 header misattributes A1 to TASK-97.1
Location: BENCHMARKS.md — "### Phase 4 — the oracle-ordering ceiling: the free-EBF lever is small (TASK-97.1 / A1)"
Impact: TASK-97.1 is item A2 (first-move-cutoff decomposition, already merged); this A1 oracle-ordering work is TASK-97.7. The permanent benchmark log — whose whole purpose here is a citable, traceable measurement — attributes A1 to the wrong sub-task, conflating it with A2.
Reproduction: git diff a360df2..2594608 -- BENCHMARKS.md ; header line references TASK-97.1.
Expected: Reference the task that produced the measurement (TASK-97.7, or the umbrella TASK-97), consistent with the task branch/commits (all "TASK-97.7").

REV-1-02 [P2] oracle_ordering example: iterations usage doc is wrong and iterations=0 panics
Location: engine/examples/oracle_ordering.rs:34-35 (usage doc) and main() line 104 (`let oracle = &passes[1];`)
Impact: The doc states the iterations arg's 0 value "leaves just the specified real->oracle two-pass". It does the opposite: oracle_profile runs `0..=iterations`, so iterations=0 produces a single reference pass, after which `passes[1]` indexes out of bounds and the harness panics. The comment is factually inaccurate (a comment must be correct standalone) and the documented boundary input crashes rather than fail-fast with a clear message.
Reproduction: RUSTFLAGS="-C target-cpu=native" cargo run --release -p engine --features oracle --example oracle_ordering -- tools/diag/bench-positions.epd 12 0 64  -> panics at index 1.
Expected: Either correct the usage doc to describe the real semantics (iterations = number of oracle passes; 1 = the two-pass; must be >= 1), and/or reject iterations < 1 with a clear assertion/message instead of an out-of-bounds panic.

Verification (on target-equivalent HEAD; 2594608..HEAD touches only the task md):
- cargo fmt --check: PASS (exit 0)
- cargo clippy --workspace --all-targets -- -D warnings: PASS (exit 0)
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): PASS (exit 0)
- cargo test --workspace: PASS (exit 0)
- Reproduced depth-12 oracle_ordering run: matches BENCHMARKS.md exactly
---

author: @claude
created: 2026-07-29 20:28
---
Rework handoff (doc fixes applied at the user's direction)
Branch: task-97.7-oracle-ordering-ceiling
Worktree: /Users/seabo/seaborg-worktrees/task-97.7-oracle-ordering-ceiling
Base: a360df285502e85d85d4a8c9a0f5be88cc54a5ee
Implementation target: 05b8ed026a9fe1f896fcdaa69270c4d36d3a24ce

Resolved REV-1-01: BENCHMARKS.md Phase 4 heading now reads "(TASK-97.7 / A1)".
Resolved REV-1-02: oracle_ordering usage doc rewritten (arg is the number of oracle passes, min 1, 1 = the two-pass); main() now asserts iterations >= 1 with a clear message, so the degenerate zero-pass input aborts cleanly instead of an out-of-bounds panic on passes[1].

Both are documentation/harness-only; no #[cfg(feature="oracle")]-gated engine logic changed, so the default build remains byte-behaviour unchanged (AC#3 intact).

Process note: these fixes were made in the review session at the user's explicit request, so implement/review separation was collapsed. The reviewer who wrote them must NOT approve them — a fresh independent review (or explicit human sign-off) is required before merge.

Verification on target 05b8ed0:
- cargo fmt --check: PASS (exit 0)
- cargo clippy --workspace --all-targets -- -D warnings: PASS (exit 0)
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): PASS (exit 0)
- cargo test --workspace: PASS (exit 0)
- iterations=0 now aborts with the guard message (no index panic); depth-12 run still reproduces BENCHMARKS.md exactly
---
<!-- COMMENTS:END -->
