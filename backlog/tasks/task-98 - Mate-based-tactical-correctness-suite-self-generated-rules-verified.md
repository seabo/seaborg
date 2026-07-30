---
id: TASK-98
title: 'Mate-based tactical-correctness suite (self-generated, rules-verified)'
status: In Review
assignee:
  - '@george'
created_date: '2026-07-29 20:28'
updated_date: '2026-07-30 10:45'
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
- [ ] #1 Generates a set of rules-verified forced-mate positions sourced from Seaborg self-play, with the mate verified by movegen/PV playout independent of the search under test (short mates additionally proven by brute-force enumeration of defender replies)
- [ ] #2 Runs a given network at normal search settings and reports the mate-find rate broken down by mate distance
- [ ] #3 Uses no external data (no tablebase, opening DB, or other engine); positions are Seaborg self-play and ground truth is the rules
- [ ] #4 Committed as a reusable, version-controlled diagnostic; tests cover the verifier (a known mate is accepted, a non-mate/stalemate is rejected)
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
<!-- COMMENTS:END -->
