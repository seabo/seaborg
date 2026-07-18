---
id: TASK-17
title: Bring the workspace to strict Clippy clean
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-18 14:44'
labels:
  - quality
  - rust
dependencies: []
references:
  - Cargo.toml
priority: medium
type: chore
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Strict Clippy currently fails and normal Clippy reports a large warning backlog across core, engine, the binary, and build scripts. Resolve or narrowly justify warnings so lint failures can become an enforced quality gate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cargo clippy --workspace --all-targets --all-features -- -D warnings passes
- [ ] #2 Any lint allowance is local and documents why the warned construct is required
- [ ] #3 Behavioral changes made during cleanup have focused regression coverage
- [ ] #4 cargo fmt --check and cargo test --workspace continue to pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Fix blocking compile error in bench targets. benches/square.rs and benches/bb.rs are undeclared in Cargo.toml (picked up by autobench discovery under the libtest harness). square.rs fails to compile because it constructs Square(34) directly, which TASK-5/TASK-30 deliberately sealed to pub(crate). Declare both as [[bench]] with harness = false and rebuild square.rs on public API. Without this, --all-targets cannot compile at all, so AC #1 is unreachable.
2. Confirm scope of --all-features: no [features] tables exist in any workspace member, so the flag is a no-op. Record this so the reviewer need not re-derive it.
3. Apply the ~74 machine-applicable lints via cargo clippy --fix, then review the generated diff hunk by hunk rather than trusting it. Treat unnecessary_cast in core/src/masks.rs, core/src/position/square.rs and core/src/bit_twiddles.rs as the highest-risk group: these are bitboard paths where a cast may be load-bearing on width or signedness.
4. Hand-fix the ~18 remaining lints. Two need reading, not mechanical application:
   - core/src/position/notation.rs:40 if_same_then_else: the KS/QS castle arms differ only in dest > orig vs dest < orig and both return true. Determine whether the duplication is a latent bug (a missing distinction) before collapsing; do not collapse blindly.
   - engine/src/search.rs:1101 unnecessary_unwrap in load_killers: hot path, so the if let rewrite must be benchmarked, not just compiled.
5. Prefer real fixes over allowances. Use a local #[allow] only where the warned construct is genuinely required, each with a comment stating why (AC #2).
6. Verify: cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo fmt --check, cargo test --workspace (AC #1, #4). Run perft to confirm the bitboard and Square cast changes are behaviour-preserving, and bench the search hot path against the base commit to confirm no regression.
7. AC #3 expectation: this sweep should be behaviour-preserving throughout. If any change does alter behaviour, add focused regression coverage for it; if nothing alters behaviour, record that explicitly as the evidence rather than leaving AC #3 silently unaddressed.
<!-- SECTION:PLAN:END -->
