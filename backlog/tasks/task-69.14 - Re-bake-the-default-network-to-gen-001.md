---
id: TASK-69.14
title: Re-bake the default network to gen-001
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-22 22:43'
updated_date: '2026-07-22 22:49'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Applied the re-baking procedure from docs/default-network.md as a content change: engine/nets/default.sbnn now holds the promoted gen-001 bytes (sha256 8ebc1381ca166774..., unchanged 394,820-byte size since the architecture is identical), BUILT_IN_NETWORK_ID moves from gen-000 to gen-001, and the pinned parameter-hash assertion moves from 0xdaf8_6bb3_d50c_ec6b to 0x3eef_37ee_f0fe_65bf. The architecture assertions (hidden width 256, qa 255, qb 64, scale 400) are unchanged because gen-001 has the same shape; only its weights differ.

The hash guard behaved exactly as its comment claims. Swapping the bytes before touching the identifier failed the test loudly and printed the actual hash, which is how the new constant was obtained. A bake that changed the weights while leaving the identifier saying gen-000 would still have played fine and misattributed every measurement made against it afterwards.

Recorded rather than asserted, from a release build of the implementation target:

  default build:            evaluator: NNUE built-in gen-001 (hidden width 256, parameter hash 0x3eef37eef0fe65bf)
  --no-default-features:    evaluator: hand-crafted evaluation

The first line is now what docs/default-network.md shows; it previously showed gen-000 and its hash, which this change would have left stale.

Provenance of the network: promoted by the reinforcement loop on the training host under TASK-69.12, gate at tc=10+0.1 concurrency 11 with SPRT elo0=0 elo1=5, crossing the upper bound after 476 games (259 wins, 159 draws, 58 losses, pentanomial [5, 15, 58, 94, 66], 0 crashes, 0 forfeits, all terminations normal) against gen-000 as baseline. Both sides were the same binary differing only in EvalFile, and the measurement was made on a build predating the embedding feature, so the baseline could not have silently been a network.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-22 22:49
---
Implementation handoff
Branch: task-69.14-rebake-gen-001
Worktree: /Users/seabo/seaborg-worktrees/task-69.14-rebake-gen-001
Base: d52a6fb1c9e1a3fd8bbcfa4bb6bbba9ba8ef7f0e
Implementation target: dd47d088e0e3e4f7e6c85fc051e68acc1105093d
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo clippy --workspace --all-targets --no-default-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (chess 50; engine 426 + 2 ignored; lichess 131; integration suites green)
- cargo test --workspace --no-default-features: pass (chess 50; engine 425 + 2 ignored; lichess 131)
- manual: release build prints 'evaluator: NNUE built-in gen-001 (hidden width 256, parameter hash 0x3eef37eef0fe65bf)'; --no-default-features build prints 'evaluator: hand-crafted evaluation'
Known failures: none

Note for the reviewer: this change is a cherry-pick of the same content from the TASK-69.12 branch, split out at the requester's direction so master can carry gen-001 while the multi-generation programme continues. That commit has been dropped from the TASK-69.12 branch so the change lands once, through this task. The Elo evidence for gen-001 was produced under TASK-69.12 and is summarised in the implementation notes here; it is not re-measured by this task, which only moves committed bytes into the binary.
---
<!-- COMMENTS:END -->
