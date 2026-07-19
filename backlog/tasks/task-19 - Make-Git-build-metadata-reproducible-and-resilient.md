---
id: TASK-19
title: Make Git build metadata reproducible and resilient
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-19 14:54'
labels:
  - build
  - metadata
dependencies: []
references:
  - build.rs
  - engine/build.rs
priority: low
type: chore
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Duplicated build scripts assume Git is installed and the source is a checkout, unwrap command and UTF-8 failures, and embed raw command output. Consolidate commit metadata with deterministic fallbacks and correct rebuild triggers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Building from a source archive or environment without Git succeeds
- [ ] #2 The embedded revision is trimmed and has a documented fallback value
- [ ] #3 Cargo reruns metadata generation when the relevant revision state changes
- [ ] #4 Duplicate build-script logic is removed or shared from one authoritative location
- [ ] #5 Package, workspace, and engine builds expose consistent version metadata
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework for review attempt 1 (REV-1-01 P1, REV-1-02 P3).

REV-1-01 - never declare a rerun-if-changed path that does not exist.
1. Establish the invariant that every emitted watch path exists at emit time. Verified against Cargo: a declared missing path leaves the unit Dirty on every subsequent build, indefinitely, because the script re-emits the same missing path each rerun.
2. Replace the two unconditional ref candidates (<common>/<ref> and <common>/packed-refs) with:
   - <common>/packed-refs only when it exists;
   - the nearest existing ancestor directory of <common>/<ref>, searched upward and bounded below by <common>/refs.
   Watching the containing directory rather than the loose ref file keeps both pack/unpack transitions observable while never naming an absent path: verified that Cargo scans a watched directory recursively, so creating, removing, renaming, or rewriting the ref file in place all mark the unit dirty, and an unchanged repository stays Fresh.
3. Factor a pure watch_paths(git_dir, common_dir, head_contents) so the layout rules are unit-testable against synthetic repositories rather than only the current checkout.
4. Rewrite the build_metadata.rs comment that documented only the beneficial half of declaring absent paths.

REV-1-02 - README says the commit appears in the UCI id response; it appears only in the startup banner. Correct the sentence to name the banner.

Tests: regression coverage for the loose layout, the fully packed layout, a packed deep branch name whose intermediate directory is absent, a detached HEAD, and the invariant that no returned path is missing.

Verification: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace, plus a no-op-rebuild-stays-Fresh check in both loose and packed clones.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Consolidated revision metadata into build_metadata.rs behind a single emit() entry point; build.rs is now a one-line call.

Deleted engine/build.rs. The engine crate never read its GIT_HASH (only a doc comment on EngineInfo.commit mentions the variable), and its '#[path = "../build_metadata.rs"]' pulled a file from outside the engine package into that package's build, so engine was not self-contained. Revision metadata now enters the engine at runtime through EngineInfo from the binary, which is the single place that embeds it.

Added rerun-if-changed for HEAD, the loose ref HEAD names, and packed-refs. The build script previously emitted no rerun-if-changed directive at all, so Cargo fell back to watching the package directory; repository metadata lives outside it, so the embedded hash silently went stale after a commit. Refs resolve against 'git rev-parse --git-common-dir' rather than '--git-dir': in a linked worktree HEAD is worktree-local while the branch file is shared, and the naive join produces a path that never exists. The emitted directives in this worktree confirm the split (HEAD under .git/worktrees/<name>/, ref under .git/refs/heads/).

Added a SEABORG_GIT_HASH override with rerun-if-env-changed, documented in README under Building. Blank or whitespace-only values fall through to discovery so an exported-but-empty variable does not embed an empty revision. A pinned build skips the repository watches, since its value cannot change with repository state.

Verified empirically rather than by inspection:
- Git failing on PATH (shim exiting 127): build succeeds, GIT_HASH=unknown, no watch paths emitted.
- Source archive with no .git ('git archive HEAD' extracted outside any repository): cargo check succeeds, GIT_HASH=unknown.
- SEABORG_GIT_HASH=pinned-1.2.3: embedded verbatim, repository watches suppressed.
- Baseline in-repo build: GIT_HASH equals HEAD, three watch paths declared.

Rework for review attempt 1.

Resolved REV-1-01. Confirmed the finding first, independently: a declared-but-missing rerun-if-changed path leaves the unit Dirty on every subsequent build, and since the script re-emits the same path each rerun the crate recompiles forever. Reproduced at the reviewed target in a packed clone ('the file .git/refs/heads/<branch> is missing' on every invocation).

The fix establishes an invariant: every emitted watch path exists at emit time. Since a branch ref is stored either loose or packed, no single file can satisfy that, so the ref's containing directory is watched instead of the ref file. Cargo scans a watched directory recursively (verified: creating, removing, and rewriting a nested file in place each mark the unit dirty), so a commit and both directions of the loose/packed transition remain observable. The climb to the nearest surviving ancestor handles a packed hierarchical name such as refs/heads/feature/work whose intermediate levels do not exist, and stops at 'refs' so the object store is never scanned — watching the Git directory itself would be both expensive and permanently dirty. packed-refs is watched additionally when present, because a packed ref can change value without touching any directory.

Factored a pure watch_paths(git_dir, common_dir, head_contents) so layout rules are testable against synthetic repositories rather than only the current checkout. Seven new tests cover the loose layout, the fully packed layout, a packed hierarchical name, the refs floor, the linked-worktree split, a detached HEAD, and an exhaustive presence/absence sweep asserting no returned path is missing.

Resolved REV-1-02. Verified the reviewer's reading: engine/src/engine.rs emits 'id name {name} {version}' with no commit, and the commit appears only in the startup banner written to the diagnostic channel. README now says so.

Empirical verification of the fix, each built twice or more:
- Packed clone (reviewer trigger b): build 2 and 3 Fresh; an empty commit reruns the script, embeds the new HEAD, then settles Fresh.
- git-init repo with no packed-refs file (reviewer trigger a): builds 2 and 3 Fresh; a commit reruns.
- Loose clone: Fresh on no-op; 'git pack-refs --all' reruns, then settles Fresh.
- Linked worktree (this one): emits worktree-local HEAD, the shared .git/refs/heads directory, and packed-refs; GIT_HASH equals HEAD.
- Re-verified the previously proven behaviour after the rewrite: git absent from PATH and a repo-less source archive both build with GIT_HASH=unknown and no watch paths, and the archive stays Fresh on rebuild; SEABORG_GIT_HASH='  pinned-1.2.3  ' embeds 'pinned-1.2.3' with watches suppressed; a whitespace-only value falls through to discovery.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 14:32
---
Implementation handoff
Branch: task-19-git-build-metadata
Worktree: /Users/seabo/seaborg-worktrees/task-19-git-build-metadata
Base: 7449461f0a2e5c4b8e0f6a1c2d3e4f5a6b7c8d9e
Implementation target: 7935a33bc28734a12a0ce09a8b7a34c557c63694
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 43 + 235 + 10 + 1 tests, 0 failed, 2 ignored (pre-existing)
Known failures: none
---

author: @claude
created: 2026-07-19 14:32
---
Correction to the handoff above: the Base line recorded a full SHA that was not the real one. The correct base commit is 74494612f016a6f44e2bd23bd73661e99a96dc3a (short 7449461, master tip when this branch was created). The Implementation target 7935a33bc28734a12a0ce09a8b7a34c557c63694 is correct and unchanged. Reviewers should diff against 7449461.
---

author: @claude
created: 2026-07-19 14:42
---
Review attempt: 1
Reviewed branch: task-19-git-build-metadata
Reviewed implementation: 7935a33bc28734a12a0ce09a8b7a34c557c63694
Base: 74494612f016a6f44e2bd23bd73661e99a96dc3a
Verdict: changes_requested

REV-1-01 [P1] Declaring a non-existent ref path keeps the binary crate permanently dirty
Location: build_metadata.rs:100-113 (revision_watch_paths), and the comment at build_metadata.rs:38-40
Impact: Blocks acceptance criterion #3. revision_watch_paths unconditionally declares BOTH
  <common>/<ref> and <common>/packed-refs, but a branch ref is stored in exactly one of those two
  forms, so in the common case one declared path does not exist. Cargo does not treat a missing
  rerun-if-changed path as "unchanged"; it marks the unit Dirty for as long as the path is absent.
  The seaborg binary crate is then fully recompiled on every cargo build/test/clippy/run with no
  source change, indefinitely. The comment at build_metadata.rs:38-40 states only the beneficial
  half of this behavior ("Cargo reruns the script when it appears"); the standing-dirty consequence
  is neither stated nor handled. This inverts AC #3: metadata regenerates when nothing changed.
  Two independently verified triggers, both routine:
    (a) any repository created by 'git init' that has never packed refs — no packed-refs file exists,
        so the crate is dirty from the very first build;
    (b) after 'git pack-refs --all' or any 'git gc' (which git also runs automatically via
        'gc --auto') — the loose ref file is removed, so that declared path goes missing.
  The seaborg repo currently has both files present, which is why declaring three watch paths looks
  correct today; the implementation notes verify that the directives are emitted, not that a no-op
  build stays fresh.
Reproduction:
  # trigger (b), on the reviewed target
  git clone /Users/seabo/seaborg /tmp/r && cd /tmp/r && git pack-refs --all
  cargo build -v 2>&1 | grep -E 'Dirty|Compiling seaborg'   # twice
  # observed, on every invocation:
  #   Dirty seaborg v0.1.0 (/tmp/r): the file `.git/refs/heads/master` is missing
  #   Compiling seaborg v0.1.0 (/tmp/r)
  # trigger (a): 'git init' a source archive and build twice; observed on every invocation:
  #   Dirty seaborg v0.1.0: the file `.git/packed-refs` is missing
Expected: A no-op rebuild is fresh in a repository whose revision has not changed, in both the
  loose-ref and packed-ref layouts, while a genuine revision change still reruns the script.
  Declaring only paths that currently exist would satisfy this, but note that the packed/loose
  transition itself must still be observable, so the fix needs to keep some path watched that
  changes when a ref is packed or unpacked.

REV-1-02 [P3] README misstates where the embedded commit is reported
Location: README.md:9-10
Impact: The added Building section says the commit is "reported by 'seaborg --uci' in the UCI 'id'
  response and in startup diagnostics". The UCI id response carries only name and version
  (engine/src/engine.rs:289 emits "id name {name} {version}"); the commit appears only in the
  startup banner (engine/src/engine.rs:117). A reader following this documentation to confirm a
  pinned SEABORG_GIT_HASH will look in the wrong place.
Reproduction:
  printf 'uci\nquit\n' | ./target/debug/seaborg --uci
  # banner: seaborg 0.1.0 by George Seabridge (commit 7935a33bc287)
  # id name seaborg 0.1.0        <- no commit
Expected: The sentence names the startup banner/diagnostics only, or the id response genuinely
  carries the commit.

Non-blocking observations (no action required on this task):
- 'cargo package -p engine' and 'cargo package -p seaborg' both fail at the base commit AND at the
  target, with "all dependencies must have a version requirement specified when packaging;
  dependency 'core' does not specify a version". This is pre-existing and unrelated to this diff,
  but it means the "Package" half of acceptance criterion #5 cannot be proven by 'cargo package'
  either before or after the change. The self-containment improvement itself is real: engine/build.rs
  and its '#[path = "../build_metadata.rs"]' escape are gone, and no engine source reads
  env!("GIT_HASH") (only a doc comment at engine/src/engine.rs:47 mentions it).
- engine/src/engine.rs:413 and :527 carry pre-existing comments citing "Acceptance #4" and
  "Acceptance #3/#5". These predate this diff and are out of scope here.

What this review confirmed as correct and proven:
- AC #1 proven twice, empirically: with git genuinely absent from PATH (symlink farm excluding git)
  the build succeeds with GIT_HASH=unknown and no watch paths; and a 'git archive HEAD' extracted to
  /tmp outside any repository builds with GIT_HASH=unknown.
- AC #2 proven: SEABORG_GIT_HASH="  pinned-1.2.3  " embeds "pinned-1.2.3" and suppresses repository
  watches; SEABORG_GIT_HASH="   " falls through to discovery; unset in a repo-less tree yields the
  documented "unknown" fallback.
- AC #4 proven: engine/build.rs deleted, build_metadata.rs is the single implementation, build.rs is
  a one-line emit() call.
- Linked-worktree handling is correct and was the right call: --git-common-dir resolves the shared
  branch file while HEAD stays worktree-local, and all three emitted paths exist in this worktree.
  The relative form returned in a main worktree ('.git') also resolves correctly, because a build
  script's cwd is the package root and Cargo resolves relative rerun-if-changed paths against it.
- The positive half of AC #3 works: touching the watched HEAD reruns the build script, and the
  first commit in a fresh repository updates the embedded hash. AC #3 is blocked only by REV-1-01.

Verification (all run by the reviewer on 7935a33, not taken from the handoff):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings,
  confirmed with a clean CARGO_TARGET_DIR
- cargo test --workspace: pass, 43 + 235 + 10 + 1 tests, 0 failed, 2 ignored (pre-existing)
- No #[allow] introduced by this diff; the #[allow(dead_code)] in tests/build_metadata.rs is
  pre-existing at the base commit.
- Benchmarks not run: the diff touches only build-time metadata emission and no move generation or
  search code.
---

author: @claude
created: 2026-07-19 14:54
---
Implementation handoff
Branch: task-19-git-build-metadata
Worktree: /Users/seabo/seaborg-worktrees/task-19-git-build-metadata
Base: 74494612f016a6f44e2bd23bd73661e99a96dc3a
Implementation target: a85e60451ce0fd7eaddbe9dfbcab56c6d339f1c1
Resolved findings: REV-1-01, REV-1-02
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 43 + 235 + 17 + 1 tests, 0 failed, 2 ignored (pre-existing)
- no-op rebuild stays Fresh: verified in a packed clone, a loose clone, a git-init repo with no packed-refs, and a repo-less source archive
- revision changes still rerun: verified by an empty commit in the packed and git-init repos, and by 'git pack-refs --all' in the loose clone
Known failures: none

Note for the reviewer: the two non-blocking observations from review attempt 1 were left alone as out of scope. 'cargo package' still fails at base and target on the pre-existing 'dependency core does not specify a version', and the 'Acceptance #3/#4/#5' comments at engine/src/engine.rs:413 and :527 predate this branch.
---
<!-- COMMENTS:END -->
