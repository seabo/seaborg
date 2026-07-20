---
id: TASK-69.1
title: 'NNUE design contract: feature set, topology, quantization, file format, loss'
status: To Do
assignee: []
created_date: '2026-07-20 19:39'
labels:
  - nnue
  - design
dependencies: []
parent_task_id: TASK-69
priority: high
ordinal: 103000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Produce the single shared contract every other NNUE subtask forks from, recorded as a decision document under docs/. Fix the decisions that are expensive to change once implementation fans out, and deliberately leave parameterizable what is cheap to vary.

Must decide and document: the input feature set (recommended starting point: perspective-doubled piece-square, 768x2 inputs, no king buckets, because incremental update is trivial and it proves the whole pipeline end to end before a costlier HalfKA-style set); network topology and the set of dimensions that stay parameterizable (hidden width, activation, output scaling); the quantization scheme (integer types, scale factors, clipped-activation semantics, saturation/overflow behaviour) since this is where the Rust and PyTorch paths most often silently diverge; the on-disk file format (a versioned header carrying architecture parameters and quantization scales, such that a loader refuses a file it does not understand rather than misinterpreting it); the training target formulation (blend of search score and game WDL outcome with a lambda, and how lambda is scheduled); and the self-play purity boundary in concrete terms.

This subtask is a decision record, not code. It is the contract subtasks .2 through .12 implement against.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A decision document under docs/ specifies feature set, topology, the parameterizable dimensions, quantization scheme, file format layout with a version header, and the training target formulation
- [ ] #2 The file format section defines a version header sufficient for a loader to reject unknown or mismatched architectures deterministically
- [ ] #3 The document states the self-play purity boundary: what internal priors are permitted and what external inputs are forbidden
<!-- AC:END -->
