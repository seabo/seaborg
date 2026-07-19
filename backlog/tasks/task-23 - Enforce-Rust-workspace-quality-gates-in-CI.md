---
id: TASK-23
title: Enforce Rust workspace quality gates in CI
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-17 17:15'
updated_date: '2026-07-19 14:29'
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
- [ ] #1 CI runs cargo fmt --check on every proposed change
- [ ] #2 CI runs cargo test --workspace in the debug profile and fails on any test failure
- [ ] #3 CI runs cargo clippy --workspace --all-targets --all-features -- -D warnings
- [ ] #4 The workflow uses a pinned or explicitly managed Rust toolchain and dependency cache inputs
- [ ] #5 Contributor documentation states the same local verification commands
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
<!-- COMMENTS:END -->
