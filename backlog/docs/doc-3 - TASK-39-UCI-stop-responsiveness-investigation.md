---
id: doc-3
title: TASK-39 UCI stop responsiveness investigation
type: other
created_date: '2026-07-18 18:28'
updated_date: '2026-07-18 18:30'
---
# TASK-39 UCI stop responsiveness investigation

## Conclusion

The current behavior is empirically fast on the tested hardware but is not acceptably bounded by the code. The measured abort-suppressed interval was below 1.1 ms in 10,000 warmed immediate-stop samples across ten representative/adversarial positions; an earlier 1,000-sample run had one 5.897 ms startup/warm-transition outlier. However, depth 1 enters quiescence, and quiet check-evasion recursion currently has no ply cap. Threefold and the fifty-move rule make it finite, but they do not establish a practically small latency bound.

Use 100 ms from receipt of stop to bestmove as this investigation's responsiveness threshold. This is a project judgement, not a number specified by UCI: UCI says stop must terminate the search as soon as possible, while runners enforce clock limits with a runner/engine-specific time margin (FastChess exposes timemargin, and can therefore be run without a grace margin). The observed values pass the threshold comfortably; the theoretical guarantee does not. A timing-only regression test would pin the sampled machine/corpus, not the actual failure mode, so this ticket does not add one.

TASK-45 specifies the cancellation fix: record a legal root fallback before honoring explicit cancellation, then allow stop/quit/EOF cancellation during depth 1. This preserves TASK-32's essential invariant (a legal move is always returned when one exists) without making UCI cancellation wait for quiescence. TASK-29 remains responsible for bounding the separate time-deadline overrun during the guaranteed minimum search.

## Code-path reasoning

- Search::run resets min_search_complete to false, then iterative_deepening always starts at depth 1.
- Search::stopping returns false while min_search_complete is false. This suppresses both the atomic cancellation flag and stop_time.
- Every check inside search, quiesce, and quiesce_evasions therefore becomes inert until the full depth-1 iteration returns and iterative_deepening sets min_search_complete.
- A depth-1 child reaches depth zero and calls quiesce. Non-check quiescence searches captures; an in-check q-node generates every legal evasion, including quiet king moves, blocks, and pawn moves, and recursively calls quiesce.
- Captures alone are bounded by remaining material. Quiet check evasions need not reduce material or advance a pawn. Threefold repetition and the fifty-move clock guarantee eventual termination along a line, but permit a very large search tree; irreversible captures/pawn moves can reset the fifty-move clock. Thus the current implementation proves finite completion, not a useful millisecond bound.
- A total quiescence/check-extension ply cap from TASK-29 would make maximum recursion depth explicit. Together with finite legal-move branching it would bound the suppressed window, and a low cap validated against the adversarial corpus should be sufficient for the time-deadline path. A cap alone would still make explicit stop wait for the capped tree, so TASK-45 is preferable for prompt cancellation semantics.

## Empirical method

Build and host:

- Commit/base: 9c4cc1896a6f9b121412bb9c5cf86b338891367d
- cargo build --release --bin seaborg
- Apple M3 Pro, Darwin arm64, rustc 1.97.1
- Harness: tools/task39_stop_probe.rb
- One persistent UCI process, Hash=1 MB, initial uci/isready warm-up.
- Before every sample: ucinewgame (clears TT generation), position, then go infinite immediately followed by stop.
- Duration starts before writing go and ends when bestmove is read, so it includes command dispatch, cancellation, the full suppressed interval, formatting, channel delivery, and pipe I/O.
- 1,000 samples per position (10,000 total). Reported depth-one node counts are UCI main-search nodes; q-node counts are not exposed in UCI telemetry and are therefore not presented as total work.

## Results

| Position class | Selection purpose | Median ms | p95 ms | Max ms | Depth-1 main nodes |
| --- | --- | ---: | ---: | ---: | ---: |
| startpos | quiet baseline | 0.044 | 0.065 | 0.930 | 21 |
| Kiwipete | dense legal/capture/castling tree | 0.239 | 0.287 | 0.757 | 49 |
| perft checks/promotions | checks, promotions, dense tactics | 0.278 | 0.318 | 1.069 | 8 |
| dense tactics | tactical middlegame | 0.047 | 0.062 | 0.273 | 41 |
| many captures | seven immediately available captures | 0.049 | 0.070 | 0.364 | 50 |
| capture chain | SEE/capture-chain stress position | 0.063 | 0.079 | 0.522 | 56 |
| in-check quiet evasions | forced entry into quiesce_evasions | 0.037 | 0.046 | 0.328 | 5 |
| mate tactics 1 | checking/mating continuations | 0.107 | 0.128 | 0.352 | 41 |
| mate tactics 2 | checking/mating continuations | 0.047 | 0.057 | 0.119 | 42 |
| check-heavy minor pieces | repeated-check potential | 0.038 | 0.046 | 0.249 | 27 |

All samples returned a legal non-null bestmove. An earlier 100-sample-per-position run observed a 5.897 ms maximum on startpos while the persistent process was transitioning from initial warm-up; the 10,000-sample table is the steady-state result, but the larger outlier is retained rather than discarded.

Quit was measured separately because it terminates the process and cannot reuse one persistent engine. Fifty warmed-handshake processes on Kiwipete received position, go infinite, quit; elapsed time from sending go/quit through process exit was min 0.673 ms, median 0.887 ms, p95 1.247 ms, max 4.102 ms. This includes worker wait plus process/thread teardown.

## Stop, quit, EOF, and teardown

engine/src/engine.rs routes stop to stop_search, which calls cancel then finish_search; finish_search blocks in SearchHandle::wait. Quit, stdin EOF, and input errors use the same stop_search call before breaking the driver loop. Replacement go, setoption, and ucinewgame also stop and join the active search first. Therefore all of these operations share the suppressed interval. Quit and EOF do not bypass or kill the worker: process teardown waits for depth 1 plus quiescence, then emits bestmove and exits. The separate quit measurements confirm the same small observed delay plus teardown overhead.

TASK-34/TASK-37 coordination remains intact: a naive change that simply re-enables cancellation before a legal fallback exists would restore bestmove 0000 on immediate stop/EOF. TASK-45 explicitly requires the legal fallback first.

## Judgement against runners

The 100 ms threshold is conservative relative to the measured sub-6 ms envelope and is short enough to feel prompt interactively. It also exceeds ordinary scheduling/pipe jitter so a regression check can distinguish engine work from noise when used diagnostically. It is not a safe clock-overrun allowance: at tc=2+0.05 the increment is only 50 ms, and FastChess's configurable timemargin means a tournament may grant no extra grace. Consequently:

- Current typical behavior is acceptable in observed play and agrees with TASK-32's zero time-forfeit self-play evidence.
- The uncapped theoretical deadline overrun is not acceptable as an invariant; TASK-29 must choose and validate an explicit quiescence/check-extension cap.
- Explicit UCI cancellation should not consume even that capped allowance once a legal fallback is available; TASK-45 addresses this directly.
- No wall-clock regression test is added here because it could pass while an adversarial unbounded tree remains possible. TASK-45 requires deterministic cancellation coverage, and TASK-29 should combine structural cap tests with corpus timing evidence.
