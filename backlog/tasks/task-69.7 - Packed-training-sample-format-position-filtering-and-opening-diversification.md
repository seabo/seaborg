---
id: TASK-69.7
title: 'Packed training-sample format, position filtering, and opening diversification'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-21 01:58'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented under engine::selfplay as three sibling modules plus CLI wiring.

format.rs — Packed on-disk record, fixed 32 bytes, little-endian, documented in
the module header:
  [0..8) occupancy word; [8..24) 32 piece nibbles (piece code 1..=12, ascending
  square order); [24] flags (bit0 stm, bits1..4 castling); [25] ep index/0xFF;
  [26] halfmove clock (u8); [27..29) fullmove (u16); [29..31) Score i16 (mate
  band preserved); [31] wdl (0/1/2). A stream is prefixed by an 8-byte header
  (magic "SBRG" + version + record size) so an incompatible file is rejected,
  not misread. PackedSample::position decodes by rebuilding a FEN and calling
  Position::from_fen, reusing the trusted parse/validation/canonicalisation path
  rather than duplicating it. SampleWriter/SampleReader stream the records;
  read() distinguishes a clean end of stream from a truncated record.

filter.rs — PositionFilter { skip_in_check, skip_best_move_capture,
  skip_opening_plies }, all configurable. retained(record) yields kept samples
  in game order, using each sample's index as its ply so the opening-ply rule is
  positional. Required adding Sample.best_move (populated by play_game from the
  search result) so the capture rule is expressible; it is not a training label
  and the packed format does not store it.

openings.rs — OpeningConfig { plies, seed }. start_for(index) plays `plies`
  uniformly-random legal moves from the start, seeded per game index by an inline
  SplitMix64 (chosen for byte-exact cross-version reproducibility, unlike a
  general RNG whose stream may change). Purity: only start_pos + legal move
  generation, no file or network input. A terminal tail is unwound so the start
  always has a legal move. plies=0 reproduces the initial position.

Wiring — SelfPlayConfig gained `opening`; run() draws start_for(game_index) per
game (start depends only on index, not scheduling). datagen CLI gained --out
(writes filtered packed samples), --opening-plies/--opening-seed, and
--keep-in-check/--keep-captures/--filter-opening-plies. Verified end to end: a
4-game run wrote 101 records = 8-byte header + 101*32 bytes exactly.

selfplay.rs was moved to selfplay/mod.rs (git rename) to host the submodules.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 01:58
---
Implementation handoff
Branch: task-69.7-packed-sample-format
Worktree: /Users/seabo/seaborg-worktrees/task-69.7-packed-sample-format
Base: 0f73ec88f5e22bb0db44839e4599077f5d4b1593
Implementation target: 6c74d0a
Resolved findings: none (first implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (all suites; engine 361 passed, 2 pre-existing ignored)
- manual: seaborg datagen --games 4 --out FILE wrote 8 + 101*32 bytes exactly
Known failures: none
---
<!-- COMMENTS:END -->
