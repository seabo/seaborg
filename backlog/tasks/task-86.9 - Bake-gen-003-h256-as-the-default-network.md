---
id: TASK-86.9
title: Bake gen-003 (h256) as the default network
status: Done
assignee:
  - '@george'
created_date: '2026-08-04 19:23'
updated_date: '2026-08-04 22:03'
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
- [ ] #1 The exact gen-003 (h256) artifact selected by TASK-86.5 is committed as the built-in default, byte-for-byte identical to the measured net (sha256 recorded and matched)
- [ ] #2 BUILT_IN_NETWORK_ID reports gen-003 and the recorded provenance (sha256 / parameter hash) matches the artifact; a plain build uses gen-003 by default
- [ ] #3 A sanity check confirms the engine loads the baked net, reports gen-003 as the active evaluator, and runs a smoke bench without regression; the source rig path + sha256 are recorded for reproducibility
- [ ] #4 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace pass
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
