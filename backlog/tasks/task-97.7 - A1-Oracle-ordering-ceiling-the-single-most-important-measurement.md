---
id: TASK-97.7
title: 'A1: Oracle-ordering ceiling (the single most important measurement)'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-29 18:46'
updated_date: '2026-07-29 19:29'
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
