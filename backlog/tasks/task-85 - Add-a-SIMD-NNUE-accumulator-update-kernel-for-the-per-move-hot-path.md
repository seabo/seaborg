---
id: TASK-85
title: Add a SIMD NNUE accumulator-update kernel for the per-move hot path
status: To Do
assignee: []
created_date: '2026-07-25 12:21'
labels: []
dependencies:
  - TASK-83
references:
  - engine/src/nnue/accumulator.rs
ordinal: 141000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The hottest NNUE path — the accumulator add/remove that runs on every make/unmake — is a plain scalar i16 loop on every architecture (`engine/src/nnue/accumulator.rs:163-189`, `add` at 172-174 and `remove` at 184-186). It has no hand-written SIMD on any target and relies entirely on the autovectorizer; on Apple Silicon this means only default-baseline NEON autovectorization. First determine whether, after the aarch64 tuning from TASK-83 lands, the autovectorized loop already captures the available gain; implement a hand-written SIMD accumulator update (NEON on aarch64, and optionally AVX2 on x86_64) behind runtime dispatch only if it measurably beats the tuned autovectorized baseline. Because this path runs per make/unmake, even a small per-call win compounds; validate with controlled measurement rather than assuming a speedup.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The autovectorized accumulator loop is benchmarked on M3 against a hand-written SIMD implementation after TASK-83 tuning is in place, with results recorded via controlled same-machine measurement
- [ ] #2 A hand-written SIMD accumulator update is implemented behind runtime feature dispatch only where it shows a measured improvement; if no consistent gain is measured, that negative result is documented and the scalar loop is retained
- [ ] #3 Any implemented kernel is numerically identical to the current scalar update and is covered by tests exercising the accelerated path
- [ ] #4 Existing NNUE evaluation and equivalence tests continue to pass on both x86_64 and aarch64
<!-- AC:END -->
