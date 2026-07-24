---
id: TASK-69.15
title: Re-bake the default network to gen-002
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-23 18:14'
updated_date: '2026-07-23 20:17'
labels:
  - nnue
  - build
dependencies: []
parent_task_id: TASK-69
ordinal: 135000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Master currently embeds gen-001 (via the merged TASK-69.14). The bootstrap programme under TASK-69.12 has since completed: gen-002 was promoted over gen-001 by +22.3 Elo, and gen-003 then failed its gate (-17.3 Elo, not promoted), so gen-002 is the programme's final best network. A plain build should carry it.

This is the same content-only re-baking procedure TASK-69.14 applied for gen-001 (docs/default-network.md), split out from TASK-69.12 so master carries the final net promptly while the programme's write-up (strength curve, cost accounting, absolute anchor measurement) is finalised separately on the TASK-69.12 branch. It satisfies the 'becomes the default evaluation' half of TASK-69.12's first acceptance criterion.

The promoted network is archived on the training host at ~/rl/run-v1/networks/gen-002.sbnn, sha256 f076dc4674eedd4295f4ef3ca999404cfb1b6c39fa2d4493029113df38addfd5, hidden width 256, same architecture as gen-000/gen-001 (only the weights differ). A direct 1000-game gauntlet measured gen-002 at ~337 Elo over the hand-crafted evaluation (two runs: 334.1 +/- 24.4 and 339.6 +/- 25.9).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 engine/nets/default.sbnn contains the promoted gen-002 network and BUILT_IN_NETWORK_ID names it, updated in the same commit
- [x] #2 The architecture and parameter-hash assertions pin gen-002, so a later bake that swaps bytes without updating the identifier still fails loudly
- [x] #3 A default release build reports the gen-002 evaluator line at startup, and docs/default-network.md shows what the binary actually prints
- [x] #4 The workspace passes fmt, strict clippy, and tests both with the embedded-net feature on and with --no-default-features
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Apply the docs/default-network.md re-baking procedure as a content change: copy the promoted gen-002 network (archived on the host, sha256 f076dc46...) over engine/nets/default.sbnn, move BUILT_IN_NETWORK_ID from gen-001 to gen-002, and move the pinned parameter-hash assertion to gen-002's hash (obtained from the guard test's failure output, which is the mechanism that catches a mismatched bake).
2. Update the evaluator line in docs/default-network.md to what the binary prints, since it currently shows gen-001.
3. Build a default release binary and record the evaluator line it actually prints; build --no-default-features and confirm it still reports the hand-crafted evaluation.
4. Run fmt, strict clippy, and workspace tests with the embedded-net feature on and with --no-default-features.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Applied the re-baking procedure as a content change. engine/nets/default.sbnn now holds gen-002 (sha256 f076dc4674eedd42..., 394,820 bytes, same size as gen-000/gen-001 since the architecture is identical). BUILT_IN_NETWORK_ID moves from gen-001 to gen-002; the pinned parameter-hash assertion moves from 0x3eef_37ee_f0fe_65bf to 0x6ad0_73be_2b68_99cb. Architecture assertions (hidden width 256, qa 255, qb 64, scale 400) unchanged.

The guard test failed loudly on the swapped bytes before the identifier was updated, printing the new hash, which is how the constant was obtained. That is the intended behaviour of the assertion.

Recorded from release builds of the implementation target, not asserted:
  default build:          evaluator: NNUE built-in gen-002 (hidden width 256, parameter hash 0x6ad073be2b6899cb)
  --no-default-features:  evaluator: hand-crafted evaluation
docs/default-network.md now shows the first line; it previously showed gen-001.

Provenance: gen-002 was promoted by the reinforcement loop under TASK-69.12, gate at tc=10+0.1 concurrency 11 SPRT elo0=0 elo1=5, crossing the upper bound after 2544 games against gen-001. gen-003 subsequently failed (-17.3 Elo, not promoted), making gen-002 the programme's final best. A direct 1000-game gauntlet against the hand-crafted evaluation measured gen-002 at 334.1 +/- 24.4 and 339.6 +/- 25.9 Elo across two runs; both sides were the same binary differing only in EvalFile, on the pre-embedding build so the baseline was genuinely hand-crafted.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-23 18:20
---
Implementation handoff
Branch: task-69.15-rebake-gen-002
Worktree: /Users/seabo/seaborg-worktrees/task-69.15-rebake-gen-002
Base: 4f9bb94d86adb078dec2568ce33777a52fdef9e4
Implementation target: 3ed63f8bbf43d51fbc0f8033dea99b25b244261e
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo clippy --workspace --all-targets --no-default-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (chess 57; engine 426 + 2 ignored; lichess 157; integration green)
- cargo test --workspace --no-default-features: pass (chess 57; engine 425 + 2 ignored; lichess 157)
- manual: default release build prints 'evaluator: NNUE built-in gen-002 (hidden width 256, parameter hash 0x6ad073be2b6899cb)'; --no-default-features prints 'evaluator: hand-crafted evaluation'
Known failures: none

Note for the reviewer: same content-only re-bake as the merged TASK-69.14, now targeting the programme's final network gen-002. The strength evidence (per-generation deltas and the ~337 Elo absolute gauntlet) was produced under TASK-69.12 and is summarised in the notes here; this task moves committed bytes into the binary and does not re-measure it.
---

author: @claude
created: 2026-07-23 20:17
---
Review attempt: 1
Reviewed branch: task-69.15-rebake-gen-002
Reviewed implementation: 3ed63f8bbf43d51fbc0f8033dea99b25b244261e
Verdict: approved

All four acceptance criteria proven objectively against the base(4f9bb94)->target(3ed63f8) diff. The diff is minimal and in scope: engine/nets/default.sbnn (binary swap), BUILT_IN_NETWORK_ID and the pinned param-hash test constant in engine/src/nnue/embedded.rs, and one evaluator line in docs/default-network.md. Post-target commit 8b3af30 changes only the task file.

AC1: default.sbnn sha256 = f076dc4674eedd4295f4ef3ca999404cfb1b6c39fa2d4493029113df38addfd5 (the promoted gen-002), 394820 bytes; BUILT_IN_NETWORK_ID = gen-002, both in 3ed63f8.
AC2: the_baked_bytes_parse_through_the_one_loader_with_the_expected_architecture parses the embedded bytes and asserts hidden_width 256, qa 255, qb 64, scale 400, and param_hash 0x6ad0_73be_2b68_99cb; a byte swap without updating the id/hash fails this test loudly.
AC3: default release binary prints 'evaluator: NNUE built-in gen-002 (hidden width 256, parameter hash 0x6ad073be2b6899cb)', matching docs/default-network.md; --no-default-features release binary prints 'evaluator: hand-crafted evaluation'.
AC4: cargo fmt --check clean; clippy --workspace --all-targets --all-features and --no-default-features both clean with -D warnings; cargo test --workspace passes (chess 57, engine 426 +2 ignored, lichess 157, integration green) and --no-default-features passes (engine 425 +2 ignored).

No movegen/search hot path is touched (data + constants + doc only), so hot-path benchmarks are not warranted. Comments in the diff are self-contained and do not reference task/finding IDs.

Verification (run on target in the task worktree):
- shasum -a 256 engine/nets/default.sbnn: f076dc46...addfd5
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo clippy --workspace --all-targets --no-default-features -- -D warnings: pass
- cargo test --workspace: pass
- cargo test --workspace --no-default-features: pass
- printf 'quit' | ./target/release/seaborg (default): gen-002 line
- printf 'quit' | ./target/release/seaborg (--no-default-features): hand-crafted evaluation
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Re-baked the default embedded network to the programme's final promoted net gen-002. engine/nets/default.sbnn now holds gen-002 (sha256 f076dc46...79fd, 394,820 bytes), BUILT_IN_NETWORK_ID and the pinned param-hash assertion (0x6ad0_73be_2b68_99cb) move to gen-002 in the same commit (3ed63f8), and the architecture assertions (256/255/64/400) are unchanged. docs/default-network.md now shows the gen-002 evaluator line. Verified: guard test parses the baked bytes and asserts the gen-002 hash; default release binary prints 'evaluator: NNUE built-in gen-002 (hidden width 256, parameter hash 0x6ad073be2b6899cb)' matching the doc, and --no-default-features prints 'hand-crafted evaluation'; fmt clean, strict clippy clean (all-features and no-default-features), tests pass in both feature configs.
<!-- SECTION:FINAL_SUMMARY:END -->
