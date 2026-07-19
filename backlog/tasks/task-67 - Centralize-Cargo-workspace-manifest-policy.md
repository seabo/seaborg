---
id: TASK-67
title: Centralize Cargo workspace manifest policy
status: Done
assignee:
  - '@george'
created_date: '2026-07-19 21:18'
updated_date: '2026-07-19 22:04'
labels:
  - architecture
dependencies: []
references:
  - docs/workspace-layout-assessment.md
priority: low
type: chore
ordinal: 85000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make the workspace's Cargo policy explicit and consistent by selecting the resolver deliberately and centralizing genuinely shared package metadata and dependency declarations. This follows the workspace-layout assessment and is separate from dependency upgrades or deduplication.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The workspace manifest explicitly selects the intended Cargo feature resolver, with the choice documented where needed
- [x] #2 Shared package metadata is declared once under workspace.package and inherited by every applicable member
- [x] #3 Dependencies shared across member packages are declared under workspace.dependencies and inherited without changing dependency features or runtime behavior
- [x] #4 Member-specific dependencies and internal path relationships remain explicit where inheritance would obscure ownership
- [x] #5 Cargo metadata succeeds and all repository-required Rust checks pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add resolver = "2" to [workspace] (explicit; matches edition-2021 default, no behavior change) with a short rationale comment.
2. Add [workspace.package] with version, authors, edition, license; inherit via .workspace = true in root, core, and engine packages.
3. Add [workspace.dependencies] for the genuinely shared crates: rand = "0.10.2" (core dep + engine dev-dep) and separator = "0.4" (root + engine). Inherit via .workspace = true, preserving default features.
4. Keep internal path deps (chess_core, engine, core) and single-member deps explicit (AC#4).
5. Verify: cargo metadata succeeds, cargo fmt --check, clippy -D warnings, cargo test --workspace. Confirm Cargo.lock unchanged (no dependency/feature drift).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Centralized workspace manifest policy across Cargo.toml, core/Cargo.toml, engine/Cargo.toml.

- AC#1 (resolver): added resolver = "2" to [workspace]. This is the edition-2021 default the workspace already used implicitly, so it is a no-op in behavior; pinning it keeps the resolver fixed independent of any future edition change. A comment records the rationale.
- AC#2 (shared metadata): added [workspace.package] with version, authors, edition, license; all three members inherit via <field>.workspace = true. version/edition/license were previously repeated in all three. authors was previously only on the root package; it is now inherited by core and engine as well (single author for the whole workspace), which is why cargo metadata now reports the author on every member. This is a deliberate consistency choice — internal crates, not published — and changes no build/runtime behavior.
- AC#3 (shared dependencies): added [workspace.dependencies] with rand = "0.10.2" (core dependency + engine dev-dependency, unified to 0.10.2 by TASK-21) and separator = "0.4" (root + engine). Both are inherited with .workspace = true using default features only, matching their prior declarations. Cargo.lock is byte-identical (0 changed lines), confirming no version or feature drift.
- AC#4 (member-specific): internal path deps (chess_core, engine, core) and single-member deps (clap, log, simple_logger, arrayvec, bitflags, unicode-segmentation, crossbeam-channel, open, criterion) remain explicit in their own manifests.

Verified inherited metadata resolves for all three members via cargo metadata.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-19 21:54
---
Implementation handoff
Branch: task-67-centralize-workspace-manifest
Worktree: /Users/seabo/seaborg-worktrees/task-67-centralize-workspace-manifest
Base: 18a4fa2326d825abcd654b9ef3d54dbedf0832b9
Implementation target: b9e89af28a454313c197534cd5f782fbf5e537fb
Resolved findings: none
Verification:
- cargo metadata --format-version=1: OK (Cargo.lock unchanged, 0 diff lines)
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (all suites green; 273/45/19/others passed, 0 failed, 2 ignored pre-existing)
Known failures: none
---

author: @george
created: 2026-07-19 21:59
---
Review attempt: 1
Reviewed branch: task-67-centralize-workspace-manifest
Reviewed implementation: b9e89af28a454313c197534cd5f782fbf5e537fb
Verdict: approved

All five acceptance criteria proven against the immutable base-to-target diff (base 18a4fa2 -> target b9e89af; branch tip 411c9e4 changes only the task file).

AC#1: resolver = "2" explicitly set in [workspace] with a rationale comment; cargo metadata resolves cleanly.
AC#2: version/authors/edition/license declared once in [workspace.package]; cargo metadata --no-deps confirms all three members (seaborg, core, engine) resolve v0.1.0 / edition 2021 / MIT / single author. (core and engine newly inherit authors — a deliberate, documented consistency choice for internal crates; no build/runtime effect.)
AC#3: rand and separator — the only cross-member third-party deps — moved to [workspace.dependencies], inherited with .workspace = true; default features preserved; Cargo.lock byte-identical (0 diff lines base..target), proving no version or feature drift.
AC#4: internal path deps (chess_core, engine, core) and single-member deps (clap, log, simple_logger, criterion, arrayvec, bitflags, unicode-segmentation, crossbeam-channel, open) remain explicit in their own manifests.
AC#5: all repository-required checks pass (below).

No #[allow] introduced. Comments are self-contained and state rationale (no task-ID/AC references). Scope limited to the three Cargo.toml files plus task lifecycle metadata. Manifest-only change with a byte-identical Cargo.lock and resolver = "2" already the implicit edition-2021 default means the dependency graph and feature set are unchanged, so hot paths are unaffected and no benchmark run was warranted.

Verification (run on the implementation target in the task worktree):
- cargo metadata --format-version=1: OK; --no-deps confirms inherited metadata for all members
- git diff --stat 18a4fa2 b9e89af -- Cargo.lock: empty (unchanged)
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings (fresh CARGO_TARGET_DIR): pass, no warnings
- cargo test --workspace: pass (45 + 273 + 19 + 1 passed; 0 failed; 2 pre-existing ignored)
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Centralized Cargo workspace manifest policy: pinned resolver = "2" (edition-2021 default, documented) in [workspace]; moved shared package metadata (version, authors, edition, license) to [workspace.package] inherited by seaborg/core/engine via <field>.workspace = true; centralized the only two cross-member third-party deps (rand, separator) under [workspace.dependencies], inherited with default features preserved; left path deps and single-member deps explicit. Verified on implementation target b9e89af: cargo metadata succeeds and resolves inherited metadata for all three members; Cargo.lock byte-identical (no version/feature drift); cargo fmt --check clean; cargo clippy --workspace --all-targets --all-features -- -D warnings clean on a fresh CARGO_TARGET_DIR; cargo test --workspace green (45+273+19+1 passed, 0 failed, 2 pre-existing ignored).
<!-- SECTION:FINAL_SUMMARY:END -->
