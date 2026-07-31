---
id: TASK-98
title: 'Mate-based tactical-correctness suite (self-generated, rules-verified)'
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-29 20:28'
updated_date: '2026-07-31 11:16'
labels:
  - nnue
  - tooling
  - search
dependencies: []
priority: high
type: feature
ordinal: 175000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Test whether the engine at normal search settings finds forced mates that provably exist, to detect search over-pruning (a nominal-depth search that prunes the winning line). Ground truth is chess rules, not an external oracle or tablebase: a forced mate is game-theoretically proven and verifiable with movegen alone.

Pipeline, entirely from our own resources: (1) source positions from our own self-play (corpus/self-play games); (2) discover candidates by deep search reporting a mate score + PV; (3) verify each mate independently of the search by playing the PV out with the perft-verified movegen and confirming checkmate (and, for short mates, a brute-force all-defender-replies proof) so the ground truth does not depend on the search under test; (4) run the engine at normal blitz settings and measure the mate-find rate by distance. A low find rate on rules-verified mates is direct evidence of over-pruning worth hundreds of Elo; a high rate rules out a gross tactical bug.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Generates a set of rules-verified forced-mate positions sourced from Seaborg self-play, with the mate verified by movegen/PV playout independent of the search under test (short mates additionally proven by brute-force enumeration of defender replies)
- [x] #2 Runs a given network at normal search settings and reports the mate-find rate broken down by mate distance
- [x] #3 Uses no external data (no tablebase, opening DB, or other engine); positions are Seaborg self-play and ground truth is the rules
- [x] #4 Committed as a reusable, version-controlled diagnostic; tests cover the verifier (a known mate is accepted, a non-mate/stalemate is rejected)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. engine::mate lib module (engine/src/mate.rs): exact forced-mate solver over chess movegen — iterative-deepening negamax that enumerates ALL defender replies (brute-force proof) and returns the minimal forced-mate distance in plies, with a node cap; independent of the heuristic search under test. Helpers: winning_first_moves (all first moves preserving a forced mate within the proven distance), verify_pv_mate (independent PV playout to a checkmate terminal), find_legal_move (UCI-string match). Unit tests: known mate-in-1/2 accepted with correct distance, stalemate rejected, non-mate rejected, PV playout accept/reject (AC#4).
2. Discovery example (engine/examples/mate_suite_gen.rs): source positions from local Seaborg self-play via engine::selfplay (deterministic: workers=1, fixed opening seed), run the exact solver on every searched position up to a configurable ply bound, dedupe by FEN, balance per distance bucket, and write the suite JSON (fen, mate_plies, winning_first_moves). No external data; ground truth is the rules (AC#1, AC#3).
3. Committed artifact suites/mate_suite.json: the generated rules-verified forced-mate set (AC#4). Document the exact generation command.
4. Measurement tool (tools/diag/mate_find_rate.py, stdlib + uci.py): load the suite, drive seaborg via UCI at normal blitz settings (go movetime, Hash 64, Threads 1) with EvalFile set to the given network, and report the mate-find rate broken down by mate distance — two criteria: plays-a-mate-preserving-move (bestmove in winning_first_moves, rules-verified) and reports-a-mate-score (AC#2).
5. tools/diag/test_mate_find_rate.py (unittest): cover suite parsing, distance bucketing, and both success criteria against synthesized suite + faked engine output (no binary needed), per repo convention.
6. Update tools/diag/README.md with the two-stage pipeline, self-play sourcing, rules-only ground truth, and the blitz-movetime rationale.
Verification: cargo fmt --check; cargo clippy --workspace --all-targets --all-features -D warnings; cargo test --workspace; python3 -m unittest for the new test; run the generator + measurement once end-to-end.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Approach validated + a critical methodology finding. The suite MUST drive the engine through the proper UCI protocol (uci.py Engine.command, waiting for bestmove): sending go then quit/EOF trips the stdin-EOF stop-abort path (TASK-37 guaranteed-first-ply fallback), so the search never runs and the engine returns a shallow non-mate move -- a false-negative trap. Verified via a raw-pipe probe that reported bestmove h1g1 on a mate-in-1, then reproduced properly via uci.py: engine finds Rb8#, Qg7#, and a back-rank mate all with 'score mate 1' at depth 14. So short-mate tactical search is sound (no gross bug). Remaining build: mine deeper forced mates (3-7) from Seaborg self-play, verify each rules-only (PV playout to a bestmove-0000 terminal + brute-force defender-reply check for short mates), then measure normal-blitz find-rate by mate distance to detect over-pruning on longer tactics.

Implemented the full diagnostic.

Design: ground truth is an exact forced-mate solver (engine::mate::MateSolver) — a complete minimax over the perft-verified move generator that credits a mate only when EVERY defender reply still loses. It shares no code with the heuristic search it diagnoses (no eval, no TT, no pruning that could drop a defence), so it is the rules-verified ground truth AC#1/#3 require; enumerating all defender replies is the brute-force proof. A structurally independent PV-playout check (playout_is_mate) replays each proven line as a second confirmation.

Pipeline:
- examples/mate_suite_gen sources positions from Seaborg self-play (engine::selfplay, single worker + fixed opening seed = deterministic) and proves forced mates on them. Resignation adjudication is disabled so won games play through to real checkmate, exposing the near-mate positions. The self-play search score is used only as a speed prefilter (skip clearly non-winning positions before the expensive proof); it never decides whether a mate is real. Output: suites/mate_suite.json (committed) = 160 rules-verified mates, balanced 40 each at mate-in-1..4, each with distance and every proven mate-preserving first move.
- tools/diag/mate_find_rate.py runs a given network via the EvalFile UCI option (so comparing networks needs no rebuild) at a normal blitz per-move budget and reports, by mate distance, how often the engine plays a mating move (checked against the proven winning moves — rules-verified, not trusting the engine score) and how often it reports the mate.

Fix found during bring-up: mate_line ran the proof and the line reconstruction on one solver, so on dense positions the proof consumed the node budget and the reconstruction returned a truncated, non-mating line. Fixed by giving the build phase a fresh budget and returning None on an incomplete line; regression-tested (mate::tests::mate_line_survives_a_budget_the_proof_alone_exhausts). Before the fix the generator's playout guard already excluded the two affected positions, so no bad data was ever admitted.

End-to-end result (embedded net, 200ms/move): plays-mate 100/100/97.5/80% and reports-mate 100/100/97.5/82.5% at mate-in-1/2/3/4 — the find-rate decline with distance is exactly the over-pruning signal the suite is for. Hand-crafted eval at depth 8 drops to 70% at mate-in-4, showing the comparative use.

Scope note: in this self-play/solver regime (40k self-play nodes, 400k solver node cap) no mate-in-5+ were both produced and proven, so the committed suite covers mate-in-1..4. The tooling supports deeper mates (raise --max-mate-plies and --node-cap); this is a parameter of regeneration, not a limitation of the method.

Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: pass (incl. 8 engine::mate tests)
- python3 -m unittest test_mate_find_rate: 17 pass
- mate_suite_gen + mate_find_rate run end-to-end against target/release/seaborg
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-30 10:45
---
Implementation handoff
Branch: task-mate-tactical-suite
Worktree: /Users/seabo/seaborg-worktrees/task-mate-tactical-suite
Base: a360df285502e85d85d4a8c9a0f5be88cc54a5ee
Implementation target: 34577bbdf40f79e1e626c688b7920a6c1e3fb147
Resolved findings: none (new work)
Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: pass
- python3 -m unittest test_mate_find_rate (tools/diag): 17 pass
- end-to-end: mate_suite_gen regenerated suites/mate_suite.json; mate_find_rate.py measured target/release/seaborg
Known failures: none
---

author: @george
created: 2026-07-31 11:16
---
Review attempt: 1
Reviewed branch: task-mate-tactical-suite
Reviewed implementation: 34577bbdf40f79e1e626c688b7920a6c1e3fb147
Verdict: approved — Ready to Merge

Immutability: base a360df28 -> target 34577bb (descends from base); the only later commit (10e91b7) is handoff metadata touching just the task .md. Worktree clean; no implementation file changed after the target.

Acceptance criteria (all proven):
- AC#1: engine::mate::MateSolver proves forced mates by exhaustive minimax over the perft-verified movegen, enumerating every defender reply (the brute-force proof) and sharing no code with the heuristic search; playout_is_mate gives an independent mechanical re-confirmation. mate_suite_gen sources positions from engine::selfplay. Independently re-verified all 160 committed positions with a freshly-written, unpruned reference solver: claimed distance, winning-move set, and playout line all match on every position (0 failures) — including the mate-in-3/4 buckets the in-tree cross-check test does not reach.
- AC#2: mate_find_rate.py drives a chosen network via EvalFile at a blitz movetime (Hash 64, Threads 1) and reports plays-mate and reports-mate rates by distance. End-to-end run @150ms/move: 100/100/97.5/77.5% plays-mate at mate-in-1..4. mate-in-1 at 100% confirms the tool drives the engine correctly through uci.py and does not trip the stdin-EOF stop-abort false negative.
- AC#3: No external data — positions are Seaborg self-play, ground truth is the rules; no tablebase/opening-DB/other engine consulted. Confirmed by reading the generator and solver.
- AC#4: Committed reusable diagnostic (mate.rs, mate_suite_gen.rs, mate_suite.json, mate_find_rate.py, README). Verifier tests: known mate accepted (accepts_a_mate_in_one, accepts_a_back_rank_mate_in_one), stalemate rejected (rejects_a_stalemate), non-mate rejected (rejects_the_opening_position), plus reference-solver agreement, budget-honesty, and playout accept/reject.

Scope/hygiene: purely additive diagnostic; no dependency changes, no #[allow] added, no comments citing task/AC/review IDs. Not on any search hot path (lib.rs only adds `pub mod mate`), so no benchmark needed.

Verification (on 34577bb, worktree /Users/seabo/seaborg-worktrees/task-mate-tactical-suite):
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: pass (8 engine::mate tests)
- python3 -m unittest test_mate_find_rate (tools/diag): 17 pass
- independent unpruned re-verification of suites/mate_suite.json: checked=160 fails=0 capped=0
- tools/diag/mate_find_rate.py end-to-end against target/release/seaborg @150ms/move: reports find-rate by distance
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds a self-contained mate-find-rate diagnostic. engine::mate::MateSolver is an exhaustive forced-mate minimax over the perft-verified move generator that credits a mate only when every defender reply loses — sharing no code (no eval/TT/pruning) with the search it diagnoses, so it is rules-only ground truth; playout_is_mate re-confirms each proven line by an independent mechanical replay. examples/mate_suite_gen sources positions from Seaborg self-play and writes suites/mate_suite.json (160 rules-verified mates, 40 each at mate-in-1..4). tools/diag/mate_find_rate.py drives a chosen network (via EvalFile) at a blitz movetime and reports the play-a-mating-move and report-mate rates by distance. Verified on target 34577bb: cargo fmt --check clean; clippy --workspace --all-targets --all-features -D warnings clean; cargo test --workspace pass (8 engine::mate tests); python3 -m unittest test_mate_find_rate 17 pass. Independently re-verified all 160 committed positions with a freshly-written unpruned solver — exact distances, winning-move sets, and playout lines all match (0 failures). End-to-end mate_find_rate.py @150ms/move reports 100/100/97.5/77.5% plays-mate at mate-in-1..4, driving the engine correctly through UCI (mate-in-1 100% rules out the EOF-trap false negative).
<!-- SECTION:FINAL_SUMMARY:END -->
