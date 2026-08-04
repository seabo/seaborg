---
id: TASK-86.9
title: Bake gen-003 (h256) as the default network
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-08-04 19:23'
updated_date: '2026-08-04 21:47'
labels:
  - nnue
  - eval
  - release
dependencies: []
references:
  - engine/src/nnue/embedded.rs
parent_task_id: TASK-86
priority: high
type: feature
ordinal: 176000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-86.5 ran the v2 architecture sweep and selected gen-003 = h256 (the gen-002 architecture retrained on the corpus-gen-002 corpus), which SPRT'd at +25.9 Elo vs the gen-002 default (tc=10+0.1, elo0=0/elo1=5). Promotion was deferred to this follow-up; the engine still ships gen-002 (engine/src/nnue/embedded.rs BUILT_IN_NETWORK_ID = gen-002), so a validated +26 Elo is currently unshipped.

Bake gen-003 as the committed default following the established re-bake procedure (TASK-69.13/.14/.15). Fetch the exact selected h256 artifact from the rig (~/rl/sweep-86.5; the precise path + sha256 are in 86.5's RESULTS.md / on-rig report), replace the committed default network asset, set BUILT_IN_NETWORK_ID to gen-003, and update the recorded provenance (sha256 / parameter hash). The baked bytes MUST match the artifact TASK-86.5 measured, byte-for-byte - shipping any other net would ship an unverified evaluation.

This is a strength-shipping change: it ships the +25.9 Elo already measured in 86.5. No re-SPRT is required (86.5 is the strength evidence); this task is about baking the correct artifact and verifying it loads and is reported correctly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The exact gen-003 (h256) artifact selected by TASK-86.5 is committed as the built-in default, byte-for-byte identical to the measured net (sha256 recorded and matched)
- [x] #2 BUILT_IN_NETWORK_ID reports gen-003 and the recorded provenance (sha256 / parameter hash) matches the artifact; a plain build uses gen-003 by default
- [x] #3 A sanity check confirms the engine loads the baked net, reports gen-003 as the active evaluator, and runs a smoke bench without regression; the source rig path + sha256 are recorded for reproducibility
- [x] #4 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Fetch the exact SPRT-measured artifact from rig (~/rl/sweep-86.5/nets/baseline__h256_crelu_v1.sbnn; sha256 e1bd2b3c00191da0f4523f3b487960c3033260988a226ee42b1976f52fe79ab1 = h256 candidate that SPRT'd +25.9 vs gen-002); verify sha256 byte-for-byte before baking.
2. Copy over engine/nets/default.sbnn (content change, keep filename).
3. Set BUILT_IN_NETWORK_ID gen-002 -> gen-003 in engine/src/nnue/embedded.rs.
4. Rebuild, read new param_hash + width from the evaluator line; update the param-hash assertion in embedded.rs tests and the evaluator example in docs/default-network.md.
5. Verify: cargo fmt --check, cargo clippy --all-features/-no-default-features -D warnings, cargo test --workspace (+ --no-default-features), and a smoke bench that loads gen-003 and reports it active with no regression.
6. Handoff to review (86.5 SPRT is the strength evidence; no re-SPRT).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Re-baked the default network to gen-003 following docs/default-network.md (same procedure as TASK-69.13/.14/.15). gen-003 = h256, the retrained gen-002 architecture on corpus-gen-002, selected by TASK-86.5 at +25.9 Elo vs gen-002 (fixed-TC SPRT).

Baked the exact 86.5-measured artifact byte-for-byte: fetched rig:~/rl/sweep-86.5/nets/baseline__h256_crelu_v1.sbnn (the candidate the h256 SPRT played, confirmed via sprt/h256/{config,report}.json), verified sha256 e1bd2b3c00191da0f4523f3b487960c3033260988a226ee42b1976f52fe79ab1 (394820 bytes) after fetch and after copying over engine/nets/default.sbnn. Set BUILT_IN_NETWORK_ID gen-002 -> gen-003, updated the param-hash assertion (0x6ad0_73be_2b68_99cb -> 0xc10b_9a80_afbb_dfb8) and the docs/default-network.md evaluator example. Architecture unchanged (width 256, qa 255, qb 64, scale 400), so only the identity fields moved.

BENCHMARKS.md line 636 (gen-002 vs hand-crafted ~337 Elo, attributed to TASK-69.15) is an archival record of a past measurement and correctly stays gen-002; the 86.5 SPRT strength record is already committed. No re-SPRT (86.5 is the evidence).

Smoke bench: driving the UCI engine with stdin held open until bestmove (piping the whole script feeds EOF, which the driver treats as a search cancel and returns a shallow move -- engine.rs:304 / the run_to_bestmove test note), gen-003 searches startpos to depth 18 (score cp 21, ~924k nps) and plays g1f3 with a natural PV; stderr reports 'NNUE built-in gen-003 (hidden width 256, parameter hash 0xc10b9a80afbbdfb8)'.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-08-04 21:33
---
Implementation handoff
Branch: task-86.9-bake-gen-003
Worktree: /Users/seabo/seaborg-worktrees/task-86.9-bake-gen-003
Base: 0fba19b
Implementation target: 1df5dea
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo clippy --workspace --all-targets --no-default-features -- -D warnings: pass
- cargo test --workspace: pass (all suites ok; the embedded hash-guard test passes on the new param hash 0xc10b9a80afbbdfb8)
- cargo test --workspace --no-default-features: pass
- Artifact provenance: rig:~/rl/sweep-86.5/nets/baseline__h256_crelu_v1.sbnn, sha256 e1bd2b3c00191da0f4523f3b487960c3033260988a226ee42b1976f52fe79ab1 (394820 bytes) == baked engine/nets/default.sbnn, byte-for-byte; it is the exact candidate the h256 SPRT played (sprt/h256/config.json + report.json)
- Smoke bench: stdin-open UCI drive, gen-003 searches startpos to depth 18 (cp 21, ~924k nps), plays g1f3; stderr reports 'NNUE built-in gen-003 (hidden width 256, parameter hash 0xc10b9a80afbbdfb8)'
Known failures: none
Reviewer note: strength evidence is TASK-86.5's SPRT (+25.9 Elo, already in BENCHMARKS.md); this task bakes the correct artifact and verifies load/report, no re-SPRT. When smoke-testing the UCI engine manually, hold stdin open until 'bestmove' -- feeding a full script pipes EOF, which the driver treats as a search cancel (engine.rs:304) and returns a shallow move (e.g. a2a3), which is not a regression.
---

author: @george
created: 2026-08-04 21:47
---
Independent review — APPROVE (code target 1df5dea)

Structural: worktree clean; 1df5dea descends from recorded base 0fba19b; only the handoff-metadata commit (e488166) sits after the target; base-to-target diff is scoped to net bytes + id/hash + docs, no accidental work.

Acceptance criteria (independently verified):
- AC1: committed default.sbnn sha256 = e1bd2b3c...9ab1, 394820 bytes — exact match to the 86.5-measured artifact.
- AC2: BUILT_IN_NETWORK_ID = gen-003; embedded param-hash guard asserts 0xc10b9a80afbbdfb8 and PASSES against the actually-built net; docs/default-network.md updated to match.
- AC3: net loads and reports gen-003 active (embedded report test passes); provenance path + sha256 recorded.
- AC4 (gate, re-run by reviewer): cargo fmt --check PASS; cargo clippy --workspace --all-targets --all-features -D warnings PASS; cargo test --workspace green except one lichess concurrency test (incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked) that failed once then passed 5/5 in isolation — pre-existing timing flake, cannot be affected by a net re-bake.

Non-blocking note: code target 1df5dea carries a Co-Authored-By: Claude trailer. Not a TASK_LIFECYCLE rule and consistent with existing master precedent; recorded as-is per author decision.

Verification:
- shasum -a 256 engine/nets/default.sbnn: e1bd2b3c...9ab1 (394820 bytes)
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (lichess flake passes 5/5 solo)

Verdict: approved. Ready to Merge on code target 1df5dea.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Re-baked the committed default network to gen-003 (h256), shipping the +25.9 Elo TASK-86.5 measured. The baked engine/nets/default.sbnn is byte-for-byte the 86.5-measured artifact (sha256 e1bd2b3c00191da0f4523f3b487960c3033260988a226ee42b1976f52fe79ab1, 394820 bytes); BUILT_IN_NETWORK_ID=gen-003 and the param-hash guard (0xc10b9a80afbbdfb8) matches the built net. Identity-only change (net bytes + id/hash + docs); architecture unchanged. Strength evidence is 86.5's SPRT; no re-SPRT required.
<!-- SECTION:FINAL_SUMMARY:END -->
