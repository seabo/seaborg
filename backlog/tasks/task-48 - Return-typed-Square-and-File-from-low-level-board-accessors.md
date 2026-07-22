---
id: TASK-48
title: Return typed Square and File from low-level board accessors
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-22 23:15'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Baseline: run benches/bb.rs, benches/movegen.rs, benches/perft.rs at the base commit and save a criterion baseline.
2. Add a File enum (chess/src/position/file.rs) with A..H, a const index(), TryFrom<u8>, Display, and a zero-cost lookup from a raw square index; export it from chess::position.
3. Change file_of_sq to return File; migrate file_bb to index FILE_BB through File::index(). Remove the TODO.
4. Change Bitboard::bsf() to return Square and panic on an empty bitboard; add Bitboard::some_bsf() -> Option<Square> as the non-panicking form, matching the existing pop_lsb_and_bit/pop_some_lsb_and_bit pair. Remove the TODO.
5. Migrate call sites: Bitboard::to_square, pop_lsb_and_bit, Iterator::next, movegen.rs check-evasion squares. No call site reconstructs a Square or File from a raw integer.
6. Add unit tests for the panicking/non-panicking bsf pair and for File.
7. Re-run the three benches against the saved baseline and record figures in the implementation notes.
8. Run cargo fmt --check, clippy -D warnings, cargo test --workspace.
<!-- SECTION:PLAN:END -->
