---
id: TASK-90
title: 'Lichess move log: add optional score and PV to the "playing <move>" line'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-28 22:25'
updated_date: '2026-07-28 22:30'
labels: []
dependencies: []
ordinal: 159000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Lichess bot mode currently logs only the played move (`game <id>: playing <move>` in lichess/src/game.rs, GameContext::on_state). Add optional score, depth, and a short principal variation to that line so game logs convey what the engine saw, without becoming verbose. The information comes from the same source UCI mode uses: per-iteration SearchProgress events on the search handle events channel. EngineMoveChooser::choose currently runs start().wait().result() and drops all Progress events (and thus every PV); the fix is to capture the last multipv==1 Progress before completion, mirroring the UCI path (engine/src/engine.rs finish_search draining events). SearchResult carries score/best_move/depth but NOT the PV, so the PV must come from that final Progress. Gate the extra output behind a single new EngineSettings boolean, defaulting on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new boolean option (e.g. log_pv) is added to EngineSettings in lichess/src/config.rs, defaults to true (enabled), and is respected when loading the TOML config; existing config tests that assert EngineSettings values are updated
- [ ] #2 When enabled, the move log line includes the score (rendered cp N / mate N via Score Display), the search depth, and a principal variation capped at 6 plies with a trailing ellipsis when truncated, e.g. `game <id>: playing e2e4 (cp 34, depth 18, pv e2e4 e7e5 g1f3 b8c6 f1b5 a7a6 ...)`
- [ ] #3 When disabled, the move log line is byte-identical to the current `game <id>: playing <move>` output
- [ ] #4 PV/score/depth are sourced from the search events channel (last multipv==1 SearchProgress), not fabricated; the played move and PV moves use consistent UCI long-algebraic formatting
- [ ] #5 The PV-length cap is a named constant in code, not a config knob
- [ ] #6 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add log_pv: bool to EngineSettings (config.rs), default true; update existing EngineSettings-literal test and assert default in partial-TOML test.
2. In game.rs, change MoveChooser::choose to return Option<Chosen> where Chosen { mov, info: Option<MoveInfo> } and MoveInfo { score, depth, pv }. EngineMoveChooser captures the events receiver before wait(), then takes the last multipv==1 Progress for score/depth/PV (mirrors UCI finish_search draining).
3. Thread config.engine.log_pv into GameContext. In on_state, when log_pv and info present, format the move line as 'playing <uci> (<score>, depth <d>, pv <=6 plies [ ...])'; otherwise byte-identical 'playing <uci>'.
4. Add LOG_PV_MAX_PLIES = 6 named constant; formatter appends ' ...' when truncated. Update FirstLegalMove test chooser to return Chosen with info None.
5. Add unit test asserting formatted PV line (cap + ellipsis) and that disabled is byte-identical. Run fmt/clippy/test.
<!-- SECTION:PLAN:END -->
