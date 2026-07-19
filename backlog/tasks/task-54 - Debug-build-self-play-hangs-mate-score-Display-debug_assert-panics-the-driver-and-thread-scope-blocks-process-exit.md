---
id: TASK-54
title: >-
  Debug-build self-play hangs: mate-score Display debug_assert panics the driver
  and thread::scope blocks process exit
status: Done
assignee:
  - '@codex'
created_date: '2026-07-18 20:11'
updated_date: '2026-07-18 23:42'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: high
type: bug
ordinal: 46000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Under debug-build self-play with a mate-rich opening book, seaborg engines wedge permanently: the match stops making progress, the engine emits no further 'bestmove', and the process never exits. Reproduced on both master (5b592eb) and the TASK-35 branch, so it is independent of TASK-35's completion-signalling fix.

Root cause (confirmed from FastChess raw protocol logs, '-log ... engine=true'):

1. `debug_assert!(plies_to_mate % 2 == 0)` at engine/src/score.rs:179, inside `impl Display for Score`, fires while the driver formats a mate score. Captured stderr: 'thread \'main\' panicked at engine/src/score.rs:179:13: assertion failed: plies_to_mate % 2 == 0'. All 8 wedged concurrency slots showed exactly this panic. So a mate score reaches Display with the opposite parity to what the assertion claims is invariant: for a negative (side-to-move-is-mated) score the plies were odd.
2. The panic is on the DRIVER/main thread (it happens inside the writeln! of a search event/outcome in engine/src/engine.rs). Unwinding leaves the run loop, and `thread::scope` then blocks forever joining the scoped stdin-reader thread, which is parked in `read_line` because the UCI runner keeps the engine's stdin open. The process therefore hangs instead of dying, and the runner waits forever for output.

The second point is why this presents as a deadlock rather than a crash, and a thread sample shows main parked under `thread::scope` with the reader in `read_line` — a signature easily mistaken for a completion deadlock.

This explains doc-2's central asymmetry for TASK-34/TASK-35: 'the debug build hangs readily; a release run of 400 games at depth 6 did not hang'. `debug_assert!` is compiled out in release, so release builds never take this path. Some or all of the debug-build hangs attributed to the lost-wakeup defect may in fact be this panic. TASK-35's lost-wakeup defect is independently real (its thread sample shows main parked in crossbeam select! with the worker exited, which is a different signature) and is fixed separately.

Reproduction: fastchess, seaborg-vs-seaborg, debug build, 'tc=inf depth=5', concurrency 8, openings from suites/wac.epd (format=epd order=random). Wedges within ~20-45 games. Mate-heavy tactical positions trigger it far faster than startpos self-play, which is why the no-book 300-game debug run did not reproduce it.

Scope: (a) fix the mate-score parity defect so the invariant genuinely holds, or correct the assertion if the invariant as written is wrong; (b) make a panic on the driver thread terminate the process instead of wedging it, so a bug surfaces as a dead engine rather than an infinite hang. Note (b) matters independently of (a): any future driver-thread panic wedges the engine the same way.

Related: TASK-36 (illegal PV moves on mate lines, Done) and TASK-12 (TT reuse and mate-score semantics, Done) both touch mate-score handling; check whether this parity violation shares a cause or was uncovered by those changes. See backlog doc-2 and TASK-35.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The mate-score parity defect is root-caused: it is established whether the debug_assert at engine/src/score.rs:179 encodes a real invariant that search violates, or is itself wrong, and the finding is recorded with the offending score value and the position/line that produces it
- [x] #2 A targeted regression test covers the failing case and fails on the pre-fix code
- [x] #3 A panic on the UCI driver thread terminates the process promptly with a non-zero exit rather than leaving it wedged in thread::scope waiting on the blocked stdin reader; a test or documented manual procedure demonstrates this
- [x] #4 Debug-build self-play (seaborg-vs-seaborg, tc=inf depth=5, concurrency>=8, openings from suites/wac.epd, at least several hundred games) completes with no hang and no panic
- [x] #5 Release-build behaviour is unchanged and existing search-correctness, mate-score, and UCI tests still pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce the debug panic from the mate-rich WAC corpus and reduce it to one deterministic position/search line, then trace the raw mate score through search and UCI conversion to decide whether the parity assertion or search semantics are wrong.
2. Add a targeted pre-fix-failing regression for that score/position and correct mate-score formatting or its producing search path while keeping wire/browser conversion aligned.
3. Refactor production UCI input ownership so the stdin reader is not scope-joined during driver unwinding, and add focused coverage plus a documented subprocess/manual non-zero-exit check.
4. Run focused mate/search/UCI tests, required Rust checks, and the specified debug FastChess WAC self-play stress run (at least several hundred games, concurrency 8); record evidence and hand off the immutable commit for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause: Score's Display parity assertions are real invariants for exact position-relative mate scores; search violated them. The failing WAC line reduces to FEN 2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61 at depth 5. Before the fix, the root reported raw score 20066 = Score::mate(34), an impossible positive/even distance, and formatting panicked. Two root-relative assumptions conflicted with the engine's position-relative convention established in TASK-12: recursive alpha/beta windows were merely negated instead of applying the inverse of neg().inc_mate(), and mate-distance pruning used root draft in local node bounds. Added Score::child_bound(), transformed main/quiescence child windows, and made non-root mate limits position-relative (mate(0)..mate(1)). The regression now returns Score::mate(7), formats as UCI mate 4, and would panic/fail on the pre-fix code.

Production launch now detaches the stdin reader and runs the UCI driver on main; normal tests retain a scoped adapter for borrowed inputs. This ensures main-thread panic unwinding cannot join a reader blocked on host-kept-open stdin. driver_panic_exits_the_process_nonzero launches an ignored subprocess probe with deliberately open input and an injected output panic; it exits non-zero within the 2-second watchdog. The companion in-process test confirms prompt unwind.

Validation:
- cargo test -p engine child_mate_windows_preserve_distance_parity: passed; pre-fix behavior reproduced manually as raw Score::mate(34)/20066 and Display parity panic at the recorded FEN/depth
- cargo test -p engine driver_panic_exits_the_process_nonzero: passed
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (core 35; engine 165 passed/2 ignored; build metadata 5; core doc-test 1)
- cargo test --release -p engine: passed (165 passed/2 ignored)
- FastChess debug self-play, WAC EPD random, depth 5, concurrency 8, 400 games: completed in two resume segments (146 + remaining 254); 27,742 bestmove responses, 0 panic/assertion log entries, final engine processes exited normally with status 0.
Known failures: none.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 22:55
---
Implementation handoff
Branch: task-54-debug-self-play-panic
Worktree: /Users/seabo/seaborg-worktrees/task-54-debug-self-play-panic
Base: ebf428924df7afef6616ad179b6c186d0faa4b6b
Implementation target: cd66d142b8f19160121bf16f306ad3805e521d41
Resolved findings: none
Verification:
- cargo test -p engine child_mate_windows_preserve_distance_parity: passed
- cargo test -p engine driver_panic_exits_the_process_nonzero: passed
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (core 35; engine 165 passed/2 ignored; build metadata 5; core doc-test 1)
- cargo test --release -p engine: passed (165 passed/2 ignored)
- FastChess debug WAC self-play, depth 5, concurrency 8, 400 games: passed; 27,742 bestmove responses, 0 panics/assertions, normal process exits
Known failures: none
---

author: @codex
created: 2026-07-18 23:31
---
Review attempt: 1
Reviewed branch: task-54-debug-self-play-panic
Reviewed implementation: cd66d142b8f19160121bf16f306ad3805e521d41
Verdict: approved

Target immutability: cd66d14 descends from base ebf4289 and is an ancestor of the branch tip; the only later commit (7d7b814) touches the task file alone. Diff scope is engine/src/{engine,score,search}.rs plus the task file, with no new `#[allow]`.

Acceptance criteria:
- #1 Root cause independently reproduced. Running the recorded FEN at depth 5 on base ebf4289 yields `Mate(34)` — positive with an even ply count — matching the recorded offending value exactly. Instrumenting the TT write site on the target confirms the notes' framing that the `Display` parity assertions are invariants for *exact* scores: parity-relaxed values are written only as `Bound::Lower` (1,703) or `Bound::Upper` (184), never `Bound::Exact` (0).
- #2 `child_mate_windows_preserve_distance_parity` fails on base (`left: Mate(34), right: Mate(7)`) and passes on target.
- #3 `driver_panic_exits_the_process_nonzero` and `driver_panic_does_not_join_a_blocked_input_reader` both pass. Mechanism confirmed: `cmdline()` calls `engine::launch` on the main thread, so an unwind exits the process rather than joining a scoped reader.
- #4 Corroborated with an independent 120-game debug self-play run (seaborg-vs-seaborg, tc=inf depth=5, concurrency 8, suites/wac.epd format=epd order=random): completed in 1:36 with 8,378 bestmove responses, zero panic or assertion entries in the raw protocol log, and exit 0. Additionally, a debug-build sweep over all 300 WAC positions at depths 4/5/6 formatted all 4,500 root progress scores through `Display` with debug assertions live: zero panics, zero parity violations.
- #5 `cargo test --release -p engine` passes (165 passed, 2 ignored). `cargo bench --bench search` (startpos depth 7) shows no repeatable regression: alternating runs gave base 40.5–41.1 us vs target 40.6–41.4 us. An initial 50.2 us target reading with a wide interval did not reproduce and was machine contention. `--bench perft`/`--bench movegen` were not run because the diff touches no move-generation code.

Verification:
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed, reconfirmed with a clean CARGO_TARGET_DIR
- cargo test --workspace: passed (engine 165 passed/2 ignored; core 35; build metadata 5; core doc-test 1)
- cargo test --release -p engine: passed (165 passed/2 ignored)
- cargo test -p engine child_mate_windows_preserve_distance_parity on base ebf4289: failed as required
- fastchess debug self-play, 120 games: no hang, no panic, exit 0
- WAC debug Display sweep, 4,500 root scores: zero panics
- cargo bench --bench search, base vs target, 3 alternating runs: no repeatable regression

Non-blocking observations, recorded for a later deliberate decision and not required for this task:
1. Step 2 no longer performs mate-distance pruning. `Score::mate(0)` (-20_100) is the minimum representable non-infinite score and `Score::mate(1)` (20_099) the maximum, so the clamp is a no-op for every real score and only maps INF_N/INF_P inward. Its `alpha >= beta` early return now fires only degenerately (observed 1,544 times in the suite, always with `alpha == Score(20_100)`). This is safe and the depth-7 bench shows no cost, but the surrounding comment still describes pruning that no longer occurs.
2. `child_bound` can yield values outside the documented mate band — observed up to +/-20_106 in quiescence, which has no clamp equivalent to Step 2, versus zero such values on base. They are consumed only as fail-soft bounds and reach neither the TT nor `Display`, but `Score`'s doc comment still states the band is 20_000–20_100, and `child_bounds_invert_parent_mate_distance_conversion` covers only in-range inputs.
3. `run` is now `#[cfg(test)]`, so pre-existing UCI driver tests exercise a test-only wrapper while production `run_detached` is covered by the two new panic tests. The shared logic lives in `drive`, so coverage is preserved in substance.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed the debug-build self-play wedge in two independent places.

Search: mate scores are position-relative, but recursive alpha/beta windows were merely negated and mate-distance pruning used root-relative `draft`. Added `Score::child_bound()` (the inverse of `neg().inc_mate()`), applied it to main and quiescence child windows, and made non-root mate limits position-relative. At the recorded WAC line `2k5/8/b1p5/Pq2r1p1/8/5PpP/3p2P1/Q2R2K1 b - - 1 61` depth 5, the root reported `Score::mate(34)` (positive, even ply count, impossible) before the fix and `Score::mate(7)` / `score mate 4` after.

UCI driver: production launch now runs the driver on main with a detached stdin reader, so a driver-thread panic can no longer be swallowed by `thread::scope` joining a reader blocked in `read_line`.

Verified on cd66d142b8f19160121bf16f306ad3805e521d41: the added regression fails on base ebf4289 (`left: Mate(34), right: Mate(7)`) and passes on target; `driver_panic_exits_the_process_nonzero` demonstrates a prompt non-zero exit; a debug-build sweep over all 300 WAC positions at depths 4/5/6 formatted 4,500 root scores through `Display` with debug assertions live and produced zero panics; a 120-game debug self-play run (WAC EPD random, depth 5, concurrency 8) completed with 8,378 bestmoves and no panic or hang; fmt, clippy `-D warnings` (clean target dir), `cargo test --workspace`, and `cargo test --release -p engine` all pass; `cargo bench --bench search` shows no repeatable regression.
<!-- SECTION:FINAL_SUMMARY:END -->
