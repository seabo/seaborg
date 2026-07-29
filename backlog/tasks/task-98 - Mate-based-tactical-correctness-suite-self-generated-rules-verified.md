---
id: TASK-98
title: 'Mate-based tactical-correctness suite (self-generated, rules-verified)'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-29 20:28'
updated_date: '2026-07-29 20:33'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Approach validated + a critical methodology finding. The suite MUST drive the engine through the proper UCI protocol (uci.py Engine.command, waiting for bestmove): sending go then quit/EOF trips the stdin-EOF stop-abort path (TASK-37 guaranteed-first-ply fallback), so the search never runs and the engine returns a shallow non-mate move -- a false-negative trap. Verified via a raw-pipe probe that reported bestmove h1g1 on a mate-in-1, then reproduced properly via uci.py: engine finds Rb8#, Qg7#, and a back-rank mate all with 'score mate 1' at depth 14. So short-mate tactical search is sound (no gross bug). Remaining build: mine deeper forced mates (3-7) from Seaborg self-play, verify each rules-only (PV playout to a bestmove-0000 terminal + brute-force defender-reply check for short mates), then measure normal-blitz find-rate by mate distance to detect over-pruning on longer tactics.
<!-- SECTION:NOTES:END -->
