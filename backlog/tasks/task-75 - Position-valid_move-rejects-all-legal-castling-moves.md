---
id: TASK-75
title: 'Position::valid_move rejects all legal castling moves'
status: Done
assignee:
  - '@george'
created_date: '2026-07-21 04:57'
updated_date: '2026-07-21 06:08'
labels:
  - chess
  - movegen
  - move-ordering
  - bug
dependencies: []
priority: medium
type: bug
ordinal: 127000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
chess::Position::valid_move (chess/src/movegen.rs, InnerMoveGen::valid_move_per_piece for the King) validates a king move against the one-square king attack table only, so it returns false for every legal castling move (both O-O and O-O-O). Confirmed deterministically: from the Kiwipete position (r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1) the generated legal move list contains e1g1 and e1c1, yet valid_move returns false for both.

This is not a legality bug in play -- the search generates and plays castles correctly via generate_castling, so no illegal move is ever emitted -- but valid_move is used as an ordering gate in two hot paths, and both silently drop legal castles:

- engine/src/search.rs:~1279: a TT best move that is a castle fails valid_move, so it is discarded AND counted as a Zobrist collision (trace.hash_collision). This loses the TT ordering hint whenever the best move is a castle and pollutes the collision diagnostic with legitimate castle moves.
- engine/src/killer.rs:82-83: a killer move that is a castle (castling is a quiet move, so it can be stored as a killer) fails valid_move and loses its ordering priority.

Net effect is a move-ordering / search-efficiency loss (and a corrupted hash-collision counter), concentrated in positions where castling is the best or a strong quiet move. Surfaced by the engine/tests/timed_selfplay.rs fixture, which is why that fixture checks legality against the generated legal move list rather than valid_move.

The fix is to teach valid_move (valid_move_helper / valid_move_per_piece) to recognise the two castle destinations for the side to move, gated on the same castling rights, occupancy, and through-check conditions that generate_castling already enforces, so valid_move agrees with the generator without weakening any domain-safety check (cf. TASK-30).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 valid_move returns true for every legal castling move and false for every illegal or unavailable one (no rights, blocked path, castling through/into/out of check), matching the generated legal move list
- [ ] #2 A regression test asserts valid_move agrees with the generated legal move list on castle moves, including the Kiwipete O-O and O-O-O case
- [ ] #3 The TT ordering path (search.rs) no longer counts a legal castle TT move as a hash collision, and a castle TT/killer move is retained as an ordering hint
- [ ] #4 Fixed-depth node counts and the required checks (cargo fmt --check, clippy -D warnings, cargo test --workspace) remain green, confirming no domain-safety check was weakened
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extract a shared predicate castle_available::<PL>(side) in InnerMoveGen that returns whether castling to a side is legal (rights, not impeded, king+rook in place, no through/into-check), mirroring castling_side exactly.
2. Refactor castling_side to call castle_available so generator and validator cannot drift.
3. In valid_move_helper, detect a CASTLE-flagged king move: reject when in check (generator never emits castles in check), map dest (relative G1/C1) to CastleType, guard on All/Quiets generation, and return castle_available result. Reject dest not matching either castle destination.
4. Add regression test asserting valid_move agrees with the generated legal move list on castles, incl. Kiwipete e1g1/e1c1, plus negative cases (no rights, blocked path, through-check).
5. Run fmt/clippy/test and confirm node counts unchanged.
<!-- SECTION:PLAN:END -->
