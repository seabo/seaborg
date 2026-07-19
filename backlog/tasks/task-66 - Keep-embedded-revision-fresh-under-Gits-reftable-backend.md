---
id: TASK-66
title: Keep embedded revision fresh under Git's reftable backend
status: To Do
assignee: []
created_date: '2026-07-19 15:13'
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
