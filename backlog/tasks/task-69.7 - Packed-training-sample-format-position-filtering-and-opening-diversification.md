---
id: TASK-69.7
title: 'Packed training-sample format, position filtering, and opening diversification'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 01:46'
labels:
  - nnue
  - datagen
dependencies:
  - TASK-69.6
parent_task_id: TASK-69
priority: high
ordinal: 109000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define and implement the compact on-disk sample format the data generator writes and the trainer reads: a packed position plus the search score plus the WDL outcome, sized for streaming hundreds of millions of samples. Add the position filtering that determines which positions are retained (for example skipping positions in check, positions whose best move is a capture, and early opening plies) and the opening diversification that keeps the game distribution broad (randomized opening plies or an internally-generated opening set, without importing external game data, to honour the purity constraint).

Format and filtering are separated from the game loop (TASK-69.6) so the encoding can be reviewed and versioned on its own; it is a data contract the Python dataloader (TASK-69.8) depends on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented packed sample format encodes position, search score, and WDL outcome, and round-trips through a reader and writer with tests
- [ ] #2 Position filtering rules are implemented and configurable, with tests asserting filtered categories are excluded
- [ ] #3 Opening diversification broadens the starting-position distribution using only internally-generated data, with no external game or position files consumed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reorganise engine::selfplay into a directory module (selfplay/mod.rs) with submodules format, filter, openings, keeping the game loop in mod.rs.
2. format.rs: fixed 32-byte packed record (occupancy u64 + 16-byte piece nibbles + flags/ep/halfmove/movenumber + i16 score + wdl byte). PackedSample pack/unpack (unpack rebuilds a FEN and calls Position::from_fen). SampleWriter/SampleReader stream with an 8-byte versioned header (magic+version+record size). Round-trip tests.
3. filter.rs: PositionFilter (skip in-check, skip best-move-capture, skip opening plies) operating per GameRecord so ply index is known; tests assert each category is excluded. Requires adding best_move to Sample so the capture rule is expressible.
4. openings.rs: deterministic OpeningGenerator using an inline splitmix64 PRNG (reproducible, no external data) that plays N random legal plies from the start position; per-game-index seeding; tests assert diversity + purity (no file IO) + non-terminal starts.
5. Wire SelfPlayConfig with an opening config and draw per-game starts in run(); extend datagen CLI to write packed output and apply filtering. Update datagen.rs.
6. Run fmt/clippy/test; write handoff.
<!-- SECTION:PLAN:END -->
