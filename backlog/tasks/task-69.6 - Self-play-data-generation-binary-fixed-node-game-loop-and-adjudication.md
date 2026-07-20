---
id: TASK-69.6
title: 'Self-play data generation binary: fixed-node game loop and adjudication'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:41'
updated_date: '2026-07-20 23:27'
labels:
  - nnue
  - datagen
dependencies:
  - TASK-69.1
parent_task_id: TASK-69
priority: high
ordinal: 108000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build the self-play data generation binary that plays games against itself at a fixed, low node budget per move (reusing the node-count search limit from TASK-64.6) and runs many games in parallel across cores, one single-threaded search per game for throughput. Iteration 0 uses the existing hand-crafted evaluation, so this binary does not depend on NNUE inference and can be developed in parallel with the inference track; a later switch selects the current network as the evaluator.

Each game records, per retained position, the search score and the eventual game outcome, and adjudicates results (win, draw, loss) with clear resign and draw rules. This task owns the game loop, parallel orchestration, and adjudication; the on-disk sample format and position filtering are TASK-69.7.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The binary self-plays games at a configurable fixed node budget per move and runs games concurrently across a configurable number of workers
- [ ] #2 Games terminate by mate, stalemate, draw rules, or adjudication, and each recorded position carries a search score and the final game outcome
- [ ] #3 Throughput (positions per second, aggregate) is measured and recorded so the training-cost estimates can be validated against reality
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine crate module engine/src/selfplay.rs (pub mod in lib.rs) owning the reusable self-play machinery, keeping the seaborg binary a thin CLI wrapper (mirrors how ui/lichess logic lives in crates).

2. Records (in-memory only; on-disk packing is TASK-69.7): Wdl{Win,Draw,Loss} (side-to-move perspective, as_f32 1.0/0.5/0.0); Sample{position, score: Score (stm perspective, cp or mate preserved), result: Wdl}; GameResult{WhiteWin,BlackWin,Draw}; Termination reason enum; GameRecord{samples, result, termination, plies}.

3. Single-game loop play_game(engine,&SearchEngine, start: Position, cfg): per ply detect terminal (checkmate/stalemate/threefold/fifty-move/insufficient-material) before searching; else run SearchEngine::start(pos.clone(), SearchLimit::Nodes(budget)).wait(); record (pos, stm-relative score); feed white-POV score to adjudicator; make best move (fallback to first legal move if None); stop on natural terminal, resign/draw adjudication, or max-ply safety cap. At end map each (pos,score) to a Sample with the game result from that position's side to move.

4. Terminal detection: reuse public chess predicates (generate<BasicMoveList,All,Legal>, in_check, in_threefold, fifty_move_rule_reached). Add a minimal insufficient-material rule (KvK, KNvK, KBvK) via piece_bb+popcnt since the engine has none; document the deliberately narrow scope. Stalemate = no legal moves and not in check.

5. Adjudication: small testable state machine over white-POV scores. Resign when the winning side's |score|>=resign_threshold_cp for resign_plies consecutive plies (mate scores exceed any cp threshold, so decisive positions adjudicate promptly). Draw when |score|<=draw_threshold_cp for draw_plies consecutive plies after draw_min_ply. Plus a hard max_plies cap -> draw.

6. Parallel orchestration run(cfg, sink)->ThroughputReport: spawn cfg.workers std::threads, each owning its own SearchEngine::new(hash) (private TT, zero contention; new_game() between games), pulling game indices from a shared AtomicUsize and sending GameRecords over an mpsc channel; the calling thread drains the channel into the sink and aggregates. Report games/positions/elapsed/positions_per_second. Start position is a parameter defaulting to start_pos; all games are identical under a deterministic node budget until opening diversification (TASK-69.7) supplies varied starts -- documented as the reason, not the ticket.

7. CLI: add Datagen(DatagenArgs) subcommand (src/cmdline.rs + src/datagen.rs) with --nodes/--workers/--games/--hash and adjudication flags; workers defaults to available_parallelism. Calls engine::selfplay::run, drops records (TASK-69.7 owns the writer), prints ThroughputReport and a termination/result breakdown to satisfy AC#3.

8. Tests (engine selfplay #[cfg(test)]): terminal_status on FENs (checkmate/stalemate/threefold/fifty-move/KvK/KNvK/KBvK/not-insufficient); adjudicator state machine (resign at right ply, draw after min ply, mate triggers resign); play_game reaches checkmate from a near-mate start with correct winner and Wdl mapping; play_game reproducibility (identical GameRecord across runs); run() with workers=2/games=4 yields 4 records and positive throughput; Wdl::as_f32 mapping.

9. Run fmt, clippy -D warnings, cargo test --workspace; hand off for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented engine::selfplay (game loop, adjudication, parallel orchestration) plus a seaborg datagen CLI subcommand. Scope boundary respected: no on-disk sample format, position filtering, or opening diversification (those are TASK-69.7); records are in-memory only and the CLI drops them after tallying.

Key decisions:
- Search driving: SearchEngine::start(pos, SearchLimit::Nodes(budget)).wait() per move (the only public path to a node budget). One SearchEngine per worker => private Arc<Table>, zero cross-worker contention; new_game() between games keeps each game reproducible in isolation. Score is taken from the side-to-move perspective (engine negamax), matching the training-target contract; centipawn and mate scores are both preserved on Sample.score, leaving the mate->cp band decision to the TASK-69.7 encoder.
- Terminal detection reuses public chess predicates (generate<BasicMoveList,All,Legal>, in_check, in_threefold, fifty_move_rule_reached). Added a minimal insufficient-material rule (KvK, KNvK, KBvK) via piece_bb+popcnt since the engine has none; harder theoretical draws are deliberately left to the fifty-move rule.
- Adjudication is a small state machine over White's-eye centipawns: resign when the winning side holds >= resign margin for N consecutive plies (mate scores exceed any cp margin, so found mates adjudicate promptly); draw when |score| <= margin for N plies after a minimum ply; plus a hard max_plies cap scored as a draw.
- Orchestration: workers pull game indices from a shared AtomicUsize and send GameRecords over an mpsc channel; the caller drains into the sink (no Send bound on the sink) and computes throughput. play_game takes the start position as a parameter so TASK-69.7 can plug diversified openings without changing the loop.

Note for the reviewer: with no diversification yet and a deterministic node budget, all games from the initial position are identical. This is by design (diversification is TASK-69.7); the run still exercises the loop, adjudication, and throughput measurement. Verified end to end: seaborg datagen --games 4 --workers 2 --nodes 20000 produced 4 games / 368 positions terminating by resignation.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 23:27
---
Implementation handoff
Branch: task-69.6-selfplay-datagen
Worktree: /Users/seabo/seaborg-worktrees/task-69.6-selfplay-datagen
Base: 6d3d4ac98a40a455959b4cea18d0b0a82b0c7867
Implementation target: 32e9989
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (chess 49, engine 318 + 2 ignored, lichess 68, integration suites green); 15 new engine::selfplay tests included
- manual: seaborg datagen --games 4 --workers 2 --nodes 20000 -> 4 games / 368 positions, resignation-adjudicated
Known failures: none
---
<!-- COMMENTS:END -->
