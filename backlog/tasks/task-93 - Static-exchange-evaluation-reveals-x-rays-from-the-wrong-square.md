---
id: TASK-93
title: Static exchange evaluation reveals x-rays from the wrong square
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-29 18:38'
updated_date: '2026-07-30 08:25'
labels:
  - search
  - ordering
dependencies: []
references:
  - engine/src/see.rs
priority: high
type: bug
ordinal: 162000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SEE mis-values many exchanges because the x-ray reveal step queries the vacated origin square instead of the from->to ray, injecting sliders that cannot legally recapture on the target.

When an attacker leaves `from`, `see` (engine/src/see.rs:78) does:

    atta_def |= self.pos.attack_defend_sliding(occ, from) & !processed;

`attack_defend_sliding` (chess/src/position/mod.rs:1100) returns every sliding piece bearing on the square passed, in ANY direction. Only sliders collinear with the from->to ray actually bear on `to`. A slider that attacks `from` along a different line (an enemy rook on `from`s rank, a bishop on the opposite diagonal) is falsely OR-ed into the attacker/defender set of `to` and can then be picked by `least_valuable_piece` as a phantom recapturer. This over-counts enemy defenders and can flip the sign of the exchange.

Reproduced against the current code (added as scratch tests, both fail):
- `k7/8/8/3p4/r3P3/8/8/7K w - - 0 1`, e4xd5 (free pawn, true SEE +100) returns Cp(0): the black rook on a4 attacks e4 along rank 4 and is treated as a defender of d5.
- `k7/8/8/8/3p4/8/8/r2R3K w - - 0 1`, Rd1xd4 (free pawn, true SEE +100) returns Cp(-400): the black rook on a1 attacks d1 along rank 1 and is treated as recapturing on d4.

The sign-flip case is the damaging one: a free winning capture computed as SEE<0 is (a) pruned outright in quiescence (see<0 vs QUIESCENCE_SEE_THRESHOLD=0, engine/src/search.rs ~3872) so it is never searched, and (b) mis-ordered good->bad in main-search capture ordering (engine/src/search.rs ~4341). Because the x-ray candidate set includes pawns, this also fires on ordinary pawn captures, and the triggering geometry (a heavy piece capturing up a file/diagonal with an enemy slider sharing the origins rank/file/diagonal; pawn captures with an enemy rook on the pawns rank) is common in real middlegames.

The conventional fix is to recompute sliders that bear on `to` after updating occupancy, e.g. `atta_def |= self.pos.attack_defend_sliding(occ, to) & !processed & occ`, which cannot introduce a slider that does not actually reach `to`; alternatively restrict the revealed set to the line through `from` and `to`. Related prior work in the same function: TASK-49 (SEE promotions).

Found via an automated correctness audit; it is the highest-impact defect that audit surfaced. This is a behavioural change touching the qsearch prune gate and capture ordering across the whole search, so it must be measured, not asserted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 When an attacker vacates `from`, SEE only reveals sliders that actually bear on `to`; no slider that attacks `from` off the from->to ray is counted as an attacker or defender of `to`
- [x] #2 Regression tests assert `k7/8/8/3p4/r3P3/8/8/7K w - - 0 1` e4xd5 returns +100 and `k7/8/8/8/3p4/8/8/r2R3K w - - 0 1` Rd1xd4 returns +100 (both currently wrong)
- [x] #3 The existing SEE suite (engine/src/see.rs tests) still passes
- [x] #4 Change measured with the TASK-27 strength-regression script, with results recorded in the implementation notes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Root cause: engine/src/see.rs:78 reveals x-ray sliders via attack_defend_sliding(occ, from) — the vacated origin — returning sliders bearing on 'from' in ANY direction, including ones off the from->to ray that cannot legally recapture on 'to'.
2. Fix: recompute the sliders bearing on 'to' under the updated occupancy — attack_defend_sliding(occ, to) & !processed — so only sliders that actually reach 'to' are folded into the attacker/defender set. Keep the may_xray gate (only pawn/bishop/rook/queen departures can uncover a slider on the ray; a knight is never collinear with 'to'; king x-ray gating is pre-existing, out of scope). Drop the now-unused 'from' local.
3. Add two regression tests: e4xd5 (=+100) and Rd1xd4 (=+100), both currently wrong.
4. Run required checks (fmt/clippy/test-workspace); confirm existing SEE suite still passes.
5. Measure with the TASK-27 strength-regression script and record results.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Root cause (engine/src/see.rs): the x-ray reveal step queried attack_defend_sliding(occ, from) — the vacated origin — which returns every slider bearing on that square in ANY direction. A slider that attacks 'from' off the from->to ray (an enemy rook on from's rank, a bishop on the opposite diagonal) was OR-ed into the attacker/defender set of 'to' and could be selected by least_valuable_piece as a phantom recapturer, over-counting defenders and sometimes flipping the sign of the exchange. A second manifestation of the same wrong-square query: along the origin's own diagonal/file the ray hit the target piece standing on 'to' as a blocker, folding the captured piece itself in as a phantom same-side defender.

Fix (one line + docs): reveal sliders that bear on 'to' under the updated occupancy — atta_def |= attack_defend_sliding(occ, to) & !processed. Only sliders collinear with the from->to ray can be genuine x-rays, and querying 'to' returns exactly those; it can never return the 'to' square itself, so the phantom-target case is gone too. Kept the may_xray gate (a departing knight is never collinear with 'to' so cannot uncover a slider; king x-ray gating is pre-existing and out of scope). The now-unused 'from' local was removed.

Tests (engine/src/see.rs):
- Added the two sign-wrong cases from the report: e4xd5 (k7/8/8/3p4/r3P3/8/8/7K w) = +100; Rd1xd4 (k7/8/8/8/3p4/8/8/r2R3K w) = +100. Both returned wrong values before the fix (Cp(0) and Cp(-400)).
- Corrected one existing expectation: the bishop battery k6q/6b1/5b2/4B3/8/2B5/1B6/K7 b, Bf6xe5, asserted Cp(0). True value is +300 — Black attacks e5 with three pieces (f6,g7,h8) vs White's two defenders (c3,b2) on the a1-h8 diagonal and nets a bishop. The old 0 was itself produced by the bug: the from-query counted the white bishop on e5 (the target) as an extra white defender, manufacturing an even 3-vs-2 exchange. Hand-traced both old (=0 via phantom target) and fixed (=+300) swap lists to confirm.
- Added an assert message (fen/from/to) to the suite loop to make future SEE mismatches diagnosable.

Verification (worktree, base b46e8bb, target 1c6d6fc):
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (clean)
- cargo test --workspace: PASS (engine 476 passed / 2 pre-existing ignored; chess 57; lichess 161; seaborg 6; integration incl. timed_selfplay). SEE it_works passes with the 2 new + 1 corrected case.

Strength (AC#4 — TASK-27 script tools/strength/strength_test.py, FastChess alpha 1.5.0):
- Command: authoritative, tc=5+0.05, concurrency 5, threads 1, hash 64MB, openings-v1.epd (sha eca449...), max-games 600. Candidate is player 1.
- Baseline binary sha256 1f4df18b...4c2aa8f2148c0ac029d21b73f6e4f (git:b46e8bb, target-cpu=native release). Candidate sha256 72bbfe1c...96c14a4018522a654a21e1874c0aacd1 (git:1c6d6fc).
- Verdict: INCONCLUSIVE at the 600-game cap. SPRT elo0=-5 elo1=0 alpha=beta=0.05, LLR=1.75, bounds [-2.94,+2.94] — the regression-hypothesis test did not reach the +2.94 PASS boundary within the cap, but LLR was positive and climbing.
- Point estimate (all 600 games, candidate vs baseline): W=222 D=243 L=135, 57.25%, Elo +50.7, 95% CI [+29.4, +72.5]. Clear gain, CI entirely above zero; 0 crashes/forfeits (the harness fails closed, so a completed authoritative result implies none).
- Interpretation: consistent with the mechanism — SEE was pruning free winning captures out of quiescence (see<0 vs QUIESCENCE_SEE_THRESHOLD=0) and mis-ordering good captures as bad in the main search; correcting it recovers real strength. Report at /tmp/seaborg-strength-93b/ (report.json, games.pgn); rerun the printed command against the same hashed binaries to reproduce.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-30 08:12
---
Implementation handoff
Branch: task-93-see-xray-wrong-square
Worktree: /Users/seabo/seaborg-worktrees/task-93-see-xray-wrong-square
Base: b46e8bb8c6b6a8168f3060bfe9c02ee37d1b3527
Implementation target: 1c6d6fcb23c67bfe66b5c71b9da621eaaaf6bd83
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS
- cargo test --workspace: PASS (engine 476 passed / 2 ignored; all crates green)
- TASK-27 strength (authoritative tc=5+0.05, 600 games, candidate=player1): verdict INCONCLUSIVE at cap (LLR=1.75, bounds [-2.94,+2.94]); point estimate W222/D243/L135, 57.25%, Elo +50.7, 95% CI [+29.4,+72.5] — clear gain, no regression
Known failures: none
---

author: @george
created: 2026-07-30 08:24
---
Review attempt: 1
Reviewed branch: task-93-see-xray-wrong-square
Reviewed implementation: 1c6d6fcb23c67bfe66b5c71b9da621eaaaf6bd83
Verdict: approved

Immutability: base b46e8bb -> target 1c6d6fc; target is an ancestor of tip abcd2a5; the only post-target change (1c6d6fc..abcd2a5) is the task .md handoff. Scope clean: base->target touches only engine/src/see.rs and the task file. No new #[allow]; code comments explain the why without citing task/finding IDs.

Correctness: the reveal step now queries attack_defend_sliding(occ, to) & !processed after vacating from. Only sliders collinear with the from->to ray bear on to, so off-ray attackers of the vacated origin can no longer be injected as phantom recapturers; querying to also can never return the target square itself, closing the phantom-target manifestation. !processed correctly excludes spent sliders whose real-board squares still show in piece_bb. Knight/king departures are gated out of the reveal (may_xray), which is sound: a knight's from/to are never collinear so it can uncover no on-ray slider (king x-ray gating is pre-existing, out of scope).

AC verification:
- AC#1: proven by code review + independent hand-trace of both report cases.
- AC#2: k7/8/8/3p4/r3P3/8/8/7K w e4xd5 = +100 and k7/8/8/8/3p4/8/8/r2R3K w Rd1xd4 = +100 added and passing (both were Cp(0)/Cp(-400) before). Hand-traced both.
- AC#3: see::tests::it_works passes. One pre-existing expectation corrected (Bf6xe5 battery k6q/6b1/5b2/4B3/8/2B5/1B6/K7 b: cp(0)->cp(300)); the old 0 was itself produced by the bug (from-query counted the e5 target bishop as an extra white defender, faking a 3v3). True value +300 hand-traced; all other existing cases unchanged and green.
- AC#4: TASK-27 strength measured and recorded — point estimate +50.7 Elo, 95% CI [+29.4,+72.5], 600 games, CI entirely above zero; consistent with the mechanism (free winning captures no longer see<0-pruned in qsearch and no longer mis-ordered good->bad).

Verification commands (target 1c6d6fc, worktree):
- cargo fmt --check: PASS
- cargo clippy --workspace --all-targets --all-features -- -D warnings: PASS (clean CARGO_TARGET_DIR /tmp/task93-clippy)
- cargo test --workspace: engine 476 passed / 2 ignored; chess/seaborg/integration green; lichess 161 passed. One lichess matchmaking timing test (incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked) timed out once under concurrent full-suite load; passes 3/3 in isolation and on a dedicated -p lichess run. Pre-existing flake, no path from see.rs.

Approved. Code target remains 1c6d6fc.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
SEE x-ray reveal fixed: engine/src/see.rs now recomputes sliders bearing on `to` under updated occupancy (attack_defend_sliding(occ, to) & !processed) instead of querying the vacated `from`, so no slider that attacks `from` off the from->to ray is folded in as a phantom recapturer. Verified: (AC#1) code + hand-traced both report cases; (AC#2/#3) see::tests::it_works passes with the two new +100 regressions (e4xd5, Rd1xd4) and one corrected pre-existing expectation (Bf6xe5 battery cp0->cp300, old 0 was itself the bug counting the e5 target as an extra defender — true value +300 hand-traced); (AC#4) TASK-27 strength recorded, +50.7 Elo 95%CI [+29.4,+72.5] over 600 games. Repo checks on target 1c6d6fc: cargo fmt --check PASS; clippy --workspace --all-targets --all-features -D warnings PASS (clean CARGO_TARGET_DIR); cargo test --workspace green (engine 476 pass/2 ignored; lichess 161 pass — one matchmaking timing test flaked only under concurrent full-suite load, passes in isolation and on a dedicated -p lichess run, no causal path from see.rs).
<!-- SECTION:FINAL_SUMMARY:END -->
