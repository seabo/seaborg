---
id: TASK-69.14
title: Re-bake the default network to gen-001
status: Done
assignee:
  - '@claude'
created_date: '2026-07-22 22:43'
updated_date: '2026-07-22 22:59'
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
- [x] #1 engine/nets/default.sbnn contains the promoted gen-001 network and BUILT_IN_NETWORK_ID names it, updated in the same commit
- [x] #2 The architecture and parameter-hash assertions pin gen-001, so a future bake that swaps bytes without updating the identifier still fails loudly
- [x] #3 A default release build reports the gen-001 evaluator line at startup, and docs/default-network.md shows what the binary actually prints
- [x] #4 The workspace passes fmt, strict clippy, and tests both with the embedded-net feature on and with --no-default-features
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

author: @claude
created: 2026-07-22 22:55
---
Review attempt: 1
Reviewed branch: task-69.14-rebake-gen-001
Reviewed implementation: dd47d088e0e3e4f7e6c85fc051e68acc1105093d
Verdict: approved

Reviewed the full base-to-target diff (3 files plus the task file). Worktree clean; the target is an ancestor of the tip; the only commit after it (808a3c0) touches the task file alone, so no implementation file changed after the reviewed SHA.

Record correction, non-blocking: the handoff comment records Base d52a6fb1c9e1a3fd8bbcfa4bb6bbba9ba8ef7f0e, which is not an object in this repository (git cat-file rejects it); it shares only the 7-char prefix with the real base. The actual base is d52a6fbc50a6061d0c5476daf10fa328306c8165, confirmed two independent ways: it is the parent of the claim commit 4315907 and the merge-base of the target with master. The diff was reviewed against that commit. This blocked neither acceptance nor safe merging, so it is corrected here rather than sent back; future handoffs should copy the full SHA rather than extend the short one.

Acceptance criteria, each proven rather than inferred:
- #1 engine/nets/default.sbnn has sha256 8ebc1381ca1667740322833488235259183bd387f8c3c7ff81b0691a70a74f51, matching the gen-001 network the description records as archived on the training host, and BUILT_IN_NETWORK_ID moves to 'gen-001' in the same commit (dd47d088).
- #2 Read the SBNN header independently of the engine (bytes at the documented offsets): hidden width 256, qa 255, qb 64, scale 400, parameter hash 0x3eef37eef0fe65bf. Those are exactly the values the test in engine/src/nnue/embedded.rs pins, and param_hash() is recomputed from the weights rather than trusted from the header, so bytes swapped without the identifier moving fail the assertion loudly.
- #3 Release build of the target, driven over stdin, prints on stderr: 'evaluator: NNUE built-in gen-001 (hidden width 256, parameter hash 0x3eef37eef0fe65bf)'. docs/default-network.md line 21 now shows that line byte-for-byte. The --no-default-features release build prints 'evaluator: hand-crafted evaluation'.
- #4 fmt, strict clippy both feature ways, and both test runs pass; see verification below.

Also checked: no stale gen-000 or 0xdaf86bb3d50cec6b reference survives anywhere outside historical Backlog records; the re-bake commit really was dropped from task-69.12-bootstrap-programme, which now introduces no competing change to these files, so the change lands once; the diff adds no #[allow]; the changed comment states why a mismatched bake matters rather than citing a task ID. No benchmark run: the diff swaps weights of identical architecture and two literals, touching no movegen or search hot path, so inference cost is unchanged by construction.

Verification (all on dd47d088):
- cargo fmt --check: pass
- CARGO_TARGET_DIR=/tmp/rev6914-clean cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, clean target dir, engine and lichess recompiled from scratch
- cargo clippy --workspace --all-targets --no-default-features -- -D warnings: pass
- cargo test --workspace: pass (engine 426 + 2 ignored, lichess 131, chess 50, integration suites green)
- cargo test --workspace --no-default-features: pass (engine 425 + 2 ignored, lichess 131, chess 50)
- shasum -a 256 engine/nets/default.sbnn: 8ebc1381ca1667740322833488235259183bd387f8c3c7ff81b0691a70a74f51
- release binary startup, default and --no-default-features: evaluator lines as quoted above
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Re-bakes the embedded default network from gen-000 to gen-001: engine/nets/default.sbnn now carries the promoted gen-001 bytes (sha256 8ebc1381ca166774..., 394,820 bytes, same architecture so the size is unchanged), BUILT_IN_NETWORK_ID names gen-001, the pinned parameter-hash assertion moves to 0x3eef_37ee_f0fe_65bf, and docs/default-network.md shows the evaluator line the binary now prints. Content change only, through the re-baking procedure that doc records; no change to how embedding works.

Verified on dd47d088: the committed file's SBNN header read independently gives hidden width 256, qa 255, qb 64, scale 400, parameter hash 0x3eef37eef0fe65bf, and its sha256 matches the network archived on the training host, so the identifier, the assertions, and the bytes agree. A release build of the target prints 'evaluator: NNUE built-in gen-001 (hidden width 256, parameter hash 0x3eef37eef0fe65bf)', byte-identical to the doc; the --no-default-features release build prints 'evaluator: hand-crafted evaluation'. cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings clean under a fresh CARGO_TARGET_DIR (so the engine crate was genuinely re-linted, not cached) and --no-default-features clean; cargo test --workspace pass (engine 426 + 2 ignored, lichess 131, chess 50) and --no-default-features pass (engine 425 + 2 ignored).
<!-- SECTION:FINAL_SUMMARY:END -->
