---
id: TASK-69.5
title: AVX2 SIMD NNUE inference with runtime dispatch and a distributable build
status: To Do
assignee: []
created_date: '2026-07-20 19:40'
labels:
  - nnue
  - inference
  - simd
  - build
dependencies:
  - TASK-69.4
parent_task_id: TASK-69
priority: medium
ordinal: 107000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a hand-written AVX2 inference path as a pure optimization of the scalar reference (TASK-69.4), selected at runtime via feature detection with the scalar path as fallback, and give the workspace a distributable build story. Today .cargo/config.toml sets target-cpu=native with no runtime dispatch, so any SIMD would be silently machine-specific and non-distributable; replace that with an explicit baseline plus runtime detection of the wider path. Declare the workspace MSRV as part of this build-story work.

A differential test asserts the SIMD path is bit-identical to the scalar path over the golden vectors and randomized positions. Correctness is defined by equality with the scalar oracle, never re-derived.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The AVX2 forward pass is bit-identical to the scalar path over the golden vectors and a randomized position set
- [ ] #2 The inference path is chosen at runtime by CPU feature detection and falls back to scalar when the wide path is unavailable
- [ ] #3 The blanket target-cpu=native default is replaced by a distributable baseline plus runtime dispatch, and the workspace MSRV is declared
<!-- AC:END -->
