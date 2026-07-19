---
id: TASK-23
title: Enforce Rust workspace quality gates in CI
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-17 17:15'
updated_date: '2026-07-19 14:35'
labels:
  - ci
  - quality
dependencies:
  - TASK-4
  - TASK-17
references:
  - AGENTS.md
  - Cargo.toml
priority: medium
type: chore
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The repository has no checked-in automation enforcing formatting, debug workspace tests, or strict lints. Add a reproducible CI workflow after the known TT test contradiction and lint backlog are resolved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 CI runs cargo fmt --check on every proposed change
- [x] #2 CI runs cargo test --workspace in the debug profile and fails on any test failure
- [x] #3 CI runs cargo clippy --workspace --all-targets --all-features -- -D warnings
- [x] #4 The workflow uses a pinned or explicitly managed Rust toolchain and dependency cache inputs
- [x] #5 Contributor documentation states the same local verification commands
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add .github/workflows/ci.yml triggered on pull_request and push to master.
2. Pin the Rust toolchain explicitly via a workflow-level RUST_VERSION env var installed with rustup (rustfmt + clippy components), avoiding third-party actions and leaving contributors' local default toolchain untouched.
3. Cache ~/.cargo/registry, ~/.cargo/git and target/ keyed on runner OS, pinned Rust version and Cargo.lock so dependency builds are reused.
4. Split into a cheap fmt job (cargo fmt --check) and a build job running clippy then tests so both build-dependent checks share one target dir and one cache.
5. Run cargo clippy --workspace --all-targets --all-features -- -D warnings and cargo test --workspace (debug profile) as separate steps so failures are attributable.
6. Document the same three local verification commands in README.md contributor guidance; AGENTS.md already states them.
7. Verify cargo fmt --check coverage empirically and confirm the three checks pass at the base commit before handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added .github/workflows/ci.yml with two jobs on pull_request and push to master:

- fmt: installs the pinned toolchain with the rustfmt component and runs cargo fmt --check. No cache, because no dependency build is involved.
- lint-and-test: installs the pinned toolchain with the clippy component, restores an actions/cache entry keyed on runner OS + pinned Rust version + hashFiles('**/Cargo.lock') over ~/.cargo/registry/{index,cache}, ~/.cargo/git/db and target, then runs strict Clippy followed by cargo test --workspace. Both build-dependent checks share one job so the test build reuses Clippy's artifacts.

Toolchain pinning uses a workflow-level RUST_VERSION env var installed with 'rustup toolchain install'. Chosen over a checked-in rust-toolchain.toml so contributors' default local toolchain is untouched, and over a third-party setup action so no external action is trusted for toolchain provenance.

CI sets RUSTFLAGS to empty, overriding the checked-in '-C target-cpu=native' in .cargo/config.toml. Hosted runners are a heterogeneous fleet, so 'native' resolves to different microarchitectures while Cargo's fingerprint records only the unchanged flag string; a cache restored onto a different host could contain unsupported instructions. Verified by grep over 'cargo check -p core -v': the default build emits target-cpu=native, and RUSTFLAGS='' emits zero occurrences. Confirmed no workspace code is conditional on CPU features (no target_feature, is_x86_feature_detected, pext or bmi2 anywhere outside .cargo/config.toml), so the override affects speed only.

Verified empirically that the documented 'cargo fmt --check' (without --all) does cover every workspace member and target, by appending misformatted functions to core/src/lib.rs, engine/src/lib.rs, tests/build_metadata.rs and benches/tt.rs and confirming all four were reported. No doc or CI change was needed to widen fmt coverage.

Confirmed build.rs degrades gracefully when git metadata is unavailable (resolve_git_hash falls back to "unknown"), so a shallow actions/checkout clone cannot fail the build.

README.md gained a Development section stating the same three commands, why the debug profile matters (78 debug_assert! sites that vanish under --release), and that the pinned CI toolchain is the reference when a local toolchain disagrees. AGENTS.md already stated the three commands and was left unchanged.

Workflow YAML parses cleanly; job and step structure confirmed by loading it with a YAML parser.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 14:29
---
Implementation handoff
Branch: task-23-ci-quality-gates
Worktree: /Users/seabo/seaborg-worktrees/task-23-ci-quality-gates
Base: cb9d36a14365103ab22ccf0e4389c73550d3973d
Implementation target: d17edfcef9067c9f5b5691d48613a927800e4f1c
Resolved findings: none
Verification:
- cargo fmt --check: pass (exit 0)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0, no warnings emitted)
- cargo test --workspace: pass (exit 0, 6 test binaries, all 'test result: ok', 0 failures)
- RUSTFLAGS override: verified locally that RUSTFLAGS='' suppresses the config's -C target-cpu=native (0 occurrences in verbose rustc invocations vs present by default)
- .github/workflows/ci.yml: parses as valid YAML; two jobs, expected steps
Known failures: none

Reviewer note: the workflow has not been executed on GitHub Actions, since this task authorizes no push. Its correctness rests on the three commands being verified to pass locally at the target commit and on the YAML parsing, not on an observed green run. The first push to the remote is what will confirm the runner-side steps (rustup install, cache restore).
---

author: @claude
created: 2026-07-19 14:35
---
Review verdict: APPROVED

Implementation target: d17edfcef9067c9f5b5691d48613a927800e4f1c
Base: cb9d36a14365103ab22ccf0e4389c73550d3973d
Branch: task-23-ci-quality-gates (tip a76f9d6 adds task metadata only; no implementation file changed after the target)

Verification run by the reviewer at the target commit, clean CARGO_TARGET_DIR:
- cargo fmt --check: exit 0
- cargo clippy --workspace --all-targets --all-features -- -D warnings: exit 0, zero warnings across core, engine and seaborg
- cargo test --workspace: exit 0, 284 passed, 0 failed, 2 ignored
No #[allow] added by the diff. No benchmark run: the diff touches only .github/workflows/ci.yml and README.md, so no move generation or search hot path is affected.

Acceptance criteria:
- #1 The fmt job runs 'cargo fmt --check' and the workflow triggers on pull_request (unrestricted by branch) plus push to master. Coverage of every workspace member and target by the bare 'cargo fmt --check' was confirmed empirically during implementation and is consistent with the command passing here.
- #2 'cargo test --workspace' runs with no --release, so the debug profile applies and debug_assert! sites stay live. A non-zero test exit fails the step and the job.
- #3 The Clippy step matches the repository-required command exactly, including -D warnings.
- #4 RUST_VERSION is pinned to 1.97.1 (a real release, 2026-07-14) and installed with 'rustup toolchain install', which is available on the ubuntu-latest image. The actions/cache key covers runner OS, the pinned version and hashFiles('**/Cargo.lock'); Cargo.lock is tracked, so that hash is non-empty and the key is meaningful.
- #5 README.md's new Development section states the same three commands as AGENTS.md, verbatim.

Points checked and cleared:
- RUSTFLAGS: "" does override build.rustflags from .cargo/config.toml. Cargo consults the config only when the env var is unset; a set-but-empty value yields an empty flag list. Confirmed no workspace code is conditional on CPU features (no target_os, target_arch, target_feature, is_x86_feature_detected, pext or bmi2 in core, engine, src, build_metadata.rs or tests), so the override changes speed only, not behavior.
- Shallow actions/checkout is safe for build.rs: the build_metadata tests exercise resolve_git_hash as a pure function over injected output and do not depend on repository git state.
- Comments in the workflow are self-contained and state their reasons rather than restating the code or citing task or finding identifiers.
- Scope is limited to the two files the task calls for.

Residual not provable under this task's authority: the workflow has never executed on GitHub Actions, since neither implementation nor review authorizes a push. Static correctness was checked as far as it can be locally -- valid YAML and job structure, real action major versions (actions/checkout@v4, actions/cache@v4), valid rustup invocations, and the env context being available in a step's with: block. The first push is what will exercise the runner-side install and cache-restore steps.

Non-blocking observations, recorded rather than raised as findings:
- concurrency.cancel-in-progress applies to master pushes too, so back-to-back merges can leave an intermediate master commit without a completed CI result. The merge skill's own re-verification covers that gap.
- Neither cargo invocation uses --locked, so a Cargo.lock that drifts from the manifests would be silently updated in CI rather than reported. Outside this task's acceptance criteria.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added .github/workflows/ci.yml enforcing the three repository-required gates on pull_request and push to master: a cheap fmt job running 'cargo fmt --check', and a lint-and-test job running strict Clippy then 'cargo test --workspace' in the debug profile over a shared target directory. The toolchain is pinned via a workflow-level RUST_VERSION installed with rustup, and actions/cache is keyed on runner OS, that pinned version and the tracked Cargo.lock. CI clears RUSTFLAGS to override the checked-in '-C target-cpu=native', which is unsafe to cache across a heterogeneous runner fleet. README.md gained a Development section stating the identical three commands. Verified at d17edfce on a clean CARGO_TARGET_DIR: cargo fmt --check (exit 0), cargo clippy --workspace --all-targets --all-features -- -D warnings (exit 0, zero warnings, all three members), cargo test --workspace (exit 0, 284 passed, 0 failed).
<!-- SECTION:FINAL_SUMMARY:END -->
