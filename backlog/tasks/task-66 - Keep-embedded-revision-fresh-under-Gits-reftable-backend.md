---
id: TASK-66
title: Keep embedded revision fresh under Git's reftable backend
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-19 15:13'
updated_date: '2026-07-19 16:37'
labels:
  - build
  - metadata
dependencies: []
priority: low
type: bug
ordinal: 84000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The build script watches .git/HEAD, the nearest existing ancestor directory of the branch ref under .git/refs, and .git/packed-refs when present. That set covers every layout Git produces with its default files backend, but not the opt-in reftable backend.

Under reftable, refs live in .git/reftable/ while .git/refs exists and stays empty, and .git/HEAD holds the placeholder 'ref: refs/heads/.invalid'. Nothing in the watched set changes when a commit lands, so Cargo leaves the crate Fresh and the binary keeps reporting the commit it was first built at. The failure is silent: the banner shows a plausible but wrong revision rather than the documented 'unknown' fallback.

Reproduced on git 2.50.1 against TASK-19's merged implementation:

  git init --ref-format=reftable . && git add -A && git commit -m init
  cargo build     # banner reports 4eb1d02abddd
  git commit --allow-empty -m two
  cargo build     # 'Fresh seaborg', banner still reports 4eb1d02abddd
  git rev-parse HEAD   # 024fd02091574dc7f27b175fb27e79d770d1e10b

Not a regression. Before TASK-19 the build script emitted no rerun-if-changed directive at all, so the hash went stale in every layout; this is a residual gap in an improvement, and reftable is not the default backend in current Git. Filed from the TASK-19 review as a latent gap rather than fixed there, because it was outside that task's scope.

Whichever approach is taken, preserve the invariant TASK-19 established: never emit a rerun-if-changed path that does not exist at emit time. Cargo does not read a missing path as 'unchanged', it holds the unit dirty while the path is absent, and because the script re-emits the same path on every rerun the crate then recompiles on every build forever. Watching .git/reftable unconditionally would reintroduce exactly that defect on files-backend repositories. Prefer detecting the backend (for example 'git rev-parse --show-ref-format' on git 2.45+) or watching .git/reftable only when it exists.

Existing coverage lives in tests/build_metadata.rs, which exercises the pure watch_paths(git_dir, common_dir, head_contents) against synthetic repository layouts; a reftable layout fits that harness.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A commit in a repository using the reftable backend reruns the build script and embeds the new revision
- [ ] #2 No rerun-if-changed path is emitted that does not exist, in reftable and files layouts alike, so no-op rebuilds stay Fresh in both
- [ ] #3 watch_paths has regression coverage for a reftable layout alongside the existing loose and packed cases
- [ ] #4 If the backend cannot be supported, the embedded revision falls back to the documented unknown value rather than a stale commit
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Detect the reftable backend structurally inside watch_paths: treat the presence of a 'reftable' directory beside the Git directory or the common directory as the signal, so no git subprocess and no minimum Git version is required.
2. Under reftable, watch the shared stack (common_dir/reftable) and, when a linked worktree has one, its per-worktree stack (git_dir/reftable). Shared refs (the branch) live in the former, per-worktree refs (HEAD) in the latter; both can move the resolved revision. Skip the refs/packed-refs logic entirely, since .git/refs is an inert placeholder file under reftable and HEAD holds a fixed sentinel.
3. Preserve the existing invariant that every emitted path exists at emit time, so files-backend repositories keep emitting exactly the same set and no-op rebuilds stay Fresh in both layouts.
4. Add watch_paths regression coverage for a reftable layout (single checkout and linked worktree) alongside the existing loose and packed cases, and extend the never-watches-a-missing-path sweep to cover reftable.
5. Verify end to end with real cargo builds in a scratch repository for each layout: commit under reftable must rebuild and embed the new revision; a no-op build must stay Fresh under both reftable and files.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Detected the reftable backend structurally rather than by asking Git. The presence of a 'reftable' directory beside the Git directory or the common directory is the signal, so watch_paths stays a pure function of a layout, needs no subprocess, and does not require Git 2.45+ for 'git rev-parse --show-ref-format'.

Under reftable both stacks are watched: the branch is a shared ref living in the common stack, while per-worktree refs including HEAD live in a linked worktree's own stack, and either can move the resolved revision. In the main checkout the two resolve to the same directory, which is deduplicated so it is never declared twice. The files-backend refs/packed-refs logic is skipped entirely under reftable, because .git/refs degrades to a placeholder *file* naming the format and HEAD holds the fixed sentinel 'ref: refs/heads/.invalid' - neither ever changes, which is precisely why the revision went stale.

Tradeoff accepted: the reftable stack is per-repository rather than per-ref, so an unrelated ref update such as a fetch or a stash also triggers a rebuild. Freshness is the property that matters here and the rebuild is of this crate alone.

End-to-end verification used a scratch crate embedding the real build.rs and build_metadata.rs, driven by real cargo and real git 2.50.1, rather than only the unit-level harness.

reftable, single checkout: first build embedded = HEAD; no-op rebuild did not recompile; a new commit recompiled and embedded the new HEAD; the following no-op rebuild did not recompile.
reftable, linked worktree: same four outcomes.
files: same four outcomes, unchanged from before.

Baseline confirmation that the fix is what changed the outcome: the same script run against master's build_metadata.rs under reftable reproduced the reported defect exactly - after a new commit the build did not recompile and the binary kept reporting the previous revision (embedded 900d3142d5c4, HEAD 03e8e9ebb23d).

One methodology note: the first end-to-end run used 'cargo build -q', which suppresses the 'Compiling' line and made the recompile assertion vacuous. It was re-run without -q; all recompile results above come from the non-quiet run.

Acceptance criterion 4 is not exercised because the backend is supported, so the unknown fallback is not reached - the revision is resolved correctly rather than degraded.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 16:37
---
Implementation handoff
Branch: task-66-reftable-revision-freshness
Worktree: /Users/seabo/seaborg-worktrees/task-66-reftable-revision-freshness
Base: c55508b3383577ed9bb62a9ebadb21fc3ecedc1f
Implementation target: b097a03
Resolved findings: none
Verification:
- cargo fmt --check: pass (exit 0)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0, no warnings)
- cargo test --workspace: pass (exit 0; 307 passed, 2 ignored, 0 failed; tests/build_metadata.rs 19 passed)
- end-to-end scratch-crate build under reftable, reftable linked worktree, and files: commit rebuilds and embeds the new HEAD; no-op rebuild stays Fresh in all three
- same script against master's build_metadata.rs under reftable: reproduces the stale revision, confirming the fix is what changed the outcome
Known failures: none
---
<!-- COMMENTS:END -->
