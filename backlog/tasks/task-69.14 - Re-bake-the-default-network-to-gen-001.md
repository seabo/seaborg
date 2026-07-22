---
id: TASK-69.14
title: Re-bake the default network to gen-001
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-22 22:43'
updated_date: '2026-07-22 22:43'
labels:
  - nnue
  - build
dependencies: []
parent_task_id: TASK-69
ordinal: 133000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The binary currently embeds gen-000, the first network the bootstrap programme promoted. The programme has since promoted gen-001, which beat gen-000 by 156.5 +/- 26.1 Elo at tc=10+0.1 under an SPRT that demanded improvement rather than mere non-regression (476 games, 259-159-58, no crashes or forfeits, run on the pinned pre-embedding build so the comparison isolates the evaluation). A plain build should carry the stronger network.

This is the content half of TASK-69.13's mechanism, applied once: the re-baking procedure recorded in docs/default-network.md, not a change to how embedding works. It is split out of TASK-69.12 (which runs the full multi-generation programme and is still mid-flight) so master can carry gen-001 now rather than waiting for the programme to finish. TASK-69.12 will re-bake again when its final generation is promoted; a re-bake is a content change to a fixed filename, so repeating it costs nothing structurally.

The promoted network is archived on the training host at ~/rl/run-v1/networks/gen-001.sbnn, sha256 8ebc1381ca1667740322833488235259183bd387f8c3c7ff81b0691a70a74f51, hidden width 256, parameter hash 0x3eef37eef0fe65bf.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 engine/nets/default.sbnn contains the promoted gen-001 network and BUILT_IN_NETWORK_ID names it, updated in the same commit
- [ ] #2 The architecture and parameter-hash assertions pin gen-001, so a future bake that swaps bytes without updating the identifier still fails loudly
- [ ] #3 A default release build reports the gen-001 evaluator line at startup, and docs/default-network.md shows what the binary actually prints
- [ ] #4 The workspace passes fmt, strict clippy, and tests both with the embedded-net feature on and with --no-default-features
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Cherry-pick the verified re-bake from the TASK-69.12 branch (d772d90): the gen-001 bytes over engine/nets/default.sbnn, BUILT_IN_NETWORK_ID moved to gen-001, the parameter-hash assertion moved to 0x3eef37eef0fe65bf, and the evaluator line in docs/default-network.md updated to what the binary prints.
2. Confirm the guard still works as intended by checking that the pinned hash and identifier were updated together, since a mismatch is the failure the assertion exists to catch.
3. Build a default release binary and record the evaluator line it actually prints, rather than asserting it.
4. Run fmt, strict clippy, and the workspace tests with the embedded-net feature on and with --no-default-features, so the hand-crafted build that measures the network's contribution stays green.
5. Drop the same commit from the TASK-69.12 branch so the change lands once, through this task.
<!-- SECTION:PLAN:END -->
