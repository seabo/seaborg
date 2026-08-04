---
id: TASK-86.9
title: Bake gen-003 (h256) as the default network
status: To Do
assignee: []
created_date: '2026-08-04 19:23'
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
