---
id: TASK-90
title: 'Lichess move log: add optional score and PV to the "playing <move>" line'
status: In Review
assignee:
  - '@george'
created_date: '2026-07-28 22:25'
updated_date: '2026-07-28 22:41'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation

- config.rs: added EngineSettings.log_pv (bool, default true). serde(default) means an absent key stays on; a present 'log_pv = false' disables. Updated the EngineSettings-literal test (engine_options_carry_hash_size) and asserted the default in partial_toml_fills_missing_fields_from_defaults; added log_pv_can_be_disabled_from_toml.
- game.rs: MoveChooser::choose now returns Option<Chosen> { mov, info: Option<MoveInfo> }. MoveInfo { score, depth, pv } is filled by EngineMoveChooser from the last multipv==1 SearchProgress, captured by cloning the events receiver before wait() and draining after — mirroring the UCI finish_search path (SearchResult has no PV). Test doubles return info: None.
- Log line: GameContext carries log_pv from config; on_state emits 'playing <uci> (<score>, depth <d>, pv <=6 plies [ ...])' only when log_pv && info is Some, else the byte-identical bare 'playing <uci>'. Score uses Score's Display (cp N / mate N); PV and played move both use UCI long-algebraic (Move::to_uci_string / Display).
- PV cap is the named constant LOG_PV_MAX_PLIES = 6 in code; formatter appends ' ...' when the PV is longer.
- Added unit tests for the annotation: 6-ply cap + ellipsis, no-ellipsis when it fits, and mate rendering.

Known failure (pre-existing, unrelated): run::tests::incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked — a threaded condvar/timing test in run.rs (untouched by this task) failed once under peak parallel load during the first cargo test --workspace, then passed on every rerun (isolated + 3 lichess-suite reruns + 2 further full-workspace runs = 6/6). The diff modifies only config.rs and game.rs; no threading, run.rs, or matchmaking code.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-28 22:40
---
Implementation handoff
Branch: task-90-lichess-move-log-pv
Worktree: /Users/seabo/seaborg-worktrees/task-90-lichess-move-log-pv
Base: e35205c044ecc265c43f250363ffef7394474e0c
Implementation target: a9a25ef
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass on rerun; new tests (config log_pv, game annotation cap/ellipsis/mate) pass
Known failures: run::tests::incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked flaked once under peak parallel load in run.rs (not modified by this task), then passed 6/6 on every rerun including two clean full-workspace runs; pre-existing timing flake unrelated to the config.rs/game.rs diff.
---
<!-- COMMENTS:END -->
