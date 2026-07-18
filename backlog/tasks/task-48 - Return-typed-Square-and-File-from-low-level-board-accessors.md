---
id: TASK-48
title: Return typed Square and File from low-level board accessors
status: To Do
assignee: []
created_date: '2026-07-18 18:30'
labels: []
dependencies: []
references:
  - core/src/bb.rs
  - core/src/position/mod.rs
priority: medium
type: chore
ordinal: 48000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two accessors return raw integers where a domain type would prevent a class of index error, continuing the direction set by TASK-5 (seal chess domain safety boundaries).

1. core/src/bb.rs:73 - Bitboard::bsf() returns u32. It should return a Square. Because bsf on an empty bitboard has no meaningful square, this likely needs both a panicking and a non-panicking (Option-returning) form so callers state which they mean. Six call sites.
2. core/src/position/mod.rs:1124 - file_of_sq returns a bare u8. It should return a dedicated File enum.

Both change signatures that ripple into move generation, so they are grouped: one benchmark check covers both. TASK-30 showed that domain-safety changes in this area can cost move-generation performance, so the benchmark evidence is a required part of the deliverable, not a formality.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bitboard::bsf returns a Square, with the empty-bitboard case handled by an explicit panicking and non-panicking pair
- [ ] #2 file_of_sq returns a dedicated File enum rather than u8
- [ ] #3 All call sites are migrated and no caller reconstructs a Square or File from a raw integer to work around the new signatures
- [ ] #4 benches/bb.rs, benches/movegen.rs and benches/perft.rs show no regression against the pre-change baseline, with figures recorded in the implementation notes
- [ ] #5 The TODOs at core/src/bb.rs:73 and core/src/position/mod.rs:1124 are removed
<!-- AC:END -->
