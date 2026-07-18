---
id: TASK-54
title: >-
  Debug-build self-play hangs: mate-score Display debug_assert panics the driver
  and thread::scope blocks process exit
status: To Do
assignee: []
created_date: '2026-07-18 20:11'
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
- [ ] #1 The mate-score parity defect is root-caused: it is established whether the debug_assert at engine/src/score.rs:179 encodes a real invariant that search violates, or is itself wrong, and the finding is recorded with the offending score value and the position/line that produces it
- [ ] #2 A targeted regression test covers the failing case and fails on the pre-fix code
- [ ] #3 A panic on the UCI driver thread terminates the process promptly with a non-zero exit rather than leaving it wedged in thread::scope waiting on the blocked stdin reader; a test or documented manual procedure demonstrates this
- [ ] #4 Debug-build self-play (seaborg-vs-seaborg, tc=inf depth=5, concurrency>=8, openings from suites/wac.epd, at least several hundred games) completes with no hang and no panic
- [ ] #5 Release-build behaviour is unchanged and existing search-correctness, mate-score, and UCI tests still pass
<!-- AC:END -->
