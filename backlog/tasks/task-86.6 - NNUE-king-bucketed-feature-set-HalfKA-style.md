---
id: TASK-86.6
title: NNUE king-bucketed feature set (HalfKA-style)
status: To Do
assignee: []
created_date: '2026-07-25 12:24'
labels:
  - nnue
dependencies:
  - TASK-86.1
  - TASK-86.5
  - TASK-82
parent_task_id: TASK-86
priority: medium
ordinal: 148000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The largest eval-quality lever in NNUE history is a king-bucketed / HalfKA-style feature set, where a piece's feature index depends on the friendly king's location, expanding the input space far beyond the current flat 768. The design contract reserves this behind a new feature_set_id and a format-version bump. Implement it as a new feature set with king-move refresh logic (a king move invalidates that perspective's accumulator and forces a rebuild), the enlarged feature transformer, matching PyTorch encoding/training/export, and Rust incremental inference with refresh. This is the biggest and riskiest lever: it should be undertaken only after the incremental accumulator (TASK-86.1) and the topology-v2 sweep (TASK-86.5) have proven the bigger-net path, and only if the TASK-82 diagnostic indicates eval quality is a dominant component of the Elo gap. Consider a factorized or modest-bucket variant first to keep memory and refresh cost manageable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new feature_set_id encodes king-dependent (HalfKA-style) features, with a format-version bump and deterministic loader rejection of mismatched architectures
- [ ] #2 The accumulator updates incrementally for non-king moves and performs a correct per-perspective refresh on king moves, proven bit-identical to a from-scratch rebuild across a representative suite
- [ ] #3 PyTorch feature encoding, training, and export match the Rust feature indexing (shared golden-vector / differential equivalence test)
- [ ] #4 A network using the new feature set is trained and evaluated by fixed-TC SPRT against the best flat-768 network, with results and attribution recorded in BENCHMARKS.md
<!-- AC:END -->
