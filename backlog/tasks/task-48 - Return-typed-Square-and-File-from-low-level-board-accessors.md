---
id: TASK-48
title: Return typed Square and File from low-level board accessors
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-22 23:42'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

- Added `chess/src/position/file.rs`: a `File` enum (A..H) with `#[repr(u8)]` discriminants equal to the low three bits of a square index, `index()`, `to_char()`, `TryFrom<u8>`, `From<File> for u8`, and `Display`. Exported as `chess::position::File`. A crate-private `File::from_low_bits` masks a raw square index to a file through a fixed 8-element table, so the conversion is total and bounds-check-free.
- `position::file_of_sq` now returns `File`; `position::file_bb` indexes `FILE_BB` through `File::index()`. TODO removed.
- `Bitboard::bsf` now returns `Square` and panics on an empty bitboard; `Bitboard::some_bsf` returns `Option<Square>` for the non-panicking case, matching the existing `pop_lsb_and_bit`/`pop_some_lsb_and_bit` naming. TODO removed.
- Migrated call sites: `Bitboard::to_square`, `Bitboard::pop_lsb_and_bit`, `Iterator::next for Bitboard`, and the two check-evasion sites in `chess/src/movegen.rs`. No call site reconstructs a `Square` or `File` from a raw integer; the single `Square(idx as u8)` construction left in the crate is inside `some_bsf` itself, which is the conversion. `benches/bb.rs` needed no change.
- `pop_lsb_and_bit`'s own `assert!` was dropped in favour of the panic `bsf` already raises, so the two entry points share one message (`empty bitboard has no lowest set bit`); the existing empty-pop test was updated to that message.
- Tests added: `bsf_names_the_lowest_set_bit` and `direct_bsf_rejects_an_empty_bitboard` in `chess/src/bb.rs`; `every_square_reports_its_own_file` in `chess/src/position/mod.rs` (all 64 squares, file index vs algebraic letter vs file mask); four `File` tests in `chess/src/position/file.rs`.

## Benchmarks

The first `--save-baseline`/`--baseline` comparison on this machine reported perft 5 at +5%, but re-running the *identical* binary swung between -1.1% and +8.5%, so criterion's sequential comparison is not usable here. Figures below are a round-robin: base (bada986) and target bench binaries built separately and run alternately in the same session, five rounds each, comparing the best point estimate of each (the least noise-inflated estimator).

| bench | base (bada986) | target | delta |
| --- | --- | --- | --- |
| perft 5 (benches/perft.rs) | 23.366 ms | 23.108 ms | -1.1% |
| generate moves (benches/movegen.rs) | 112.60 ns | 109.74 ns | -2.5% |
| bsf (benches/bb.rs) | 338.55 ps | 272.40 ps | -19.5% |
| iterate set bits (benches/bb.rs) | 13.472 ns | 13.637 ns | +1.2% |

Medians across the five rounds agree: perft 23.417 -> 23.574 ms, generate moves 114.05 -> 113.02 ns, bsf 341.01 -> 301.62 ps. No regression; perft and movegen are at parity within this machine's noise and `bsf` itself is faster.

An earlier draft of `some_bsf` tested `is_empty()` before counting trailing zeros. That measured a consistent 2-3% slower on `iterate set bits` across six round-robin rounds, so it was rewritten to match on the trailing-zero count against 64 — the shape the old code had. The comment in `some_bsf` records why.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-22 23:41
---
Implementation handoff
Branch: task-48-typed-square-file
Worktree: /Users/seabo/seaborg-worktrees/task-48-typed-square-file
Base: bada986c932ba144835cc941d29263f64f2a22f6
Implementation target: 7d938139a41f26ca1d1b8f1c72d84f2bb0e12a8b
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 654 passed, 0 failed, 2 ignored
- round-robin bench base vs target (benches/bb.rs, benches/movegen.rs, benches/perft.rs), 5 alternating rounds: no regression; figures in the implementation notes
Known failures: none
---

author: @claude
created: 2026-07-22 23:42
---
Correction to the handoff above: the implementation target sha is 7d938139023bbfbf2acedcee40652a1373cf6054. Everything else in that comment stands.
---
<!-- COMMENTS:END -->
