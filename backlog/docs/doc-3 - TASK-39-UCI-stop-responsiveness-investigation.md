---
id: doc-3
title: TASK-39 UCI stop responsiveness investigation
type: other
created_date: '2026-07-18 18:28'
updated_date: '2026-07-18 20:10'
---
# TASK-39 UCI stop responsiveness investigation

## Conclusion

The abort-suppressed window is **empirically small but structurally unbounded**, and the two facts
have different causes. Measured latency from `go infinite` + immediate `stop` to `bestmove` stayed
under 6 ms across 16,000 samples on 16 positions, including positions selected specifically to be
adversarial. But that smallness is produced by alpha-beta, stand-pat and TT pruning, not by any
bound in the suppression logic: the ply-1 quiescence tree the window must run to completion is
reachable to at least 55 ply and well past 20 million nodes.

Threshold used: **100 ms** from receipt of `stop` to `bestmove`. This is a project judgement, not a
UCI-specified number. UCI says `stop` must terminate the search as soon as possible; runners enforce
clock limits with a runner-specific margin (FastChess exposes `timemargin`, and can be configured
with no grace at all). Observed behavior passes this threshold by more than an order of magnitude.
The guarantee does not exist.

The investigation therefore ends with implementation tickets rather than a timing regression test. A
wall-clock test would pin this machine and this corpus while the actual failure mode — an
adversarial position where pruning fails to contain the ply-1 q-tree — remained possible.

**TASK-45** specifies the cancellation fix: record a legal root fallback, then honor explicit
cancellation during depth 1. This preserves TASK-32's invariant (a legal move is always returned)
without making UCI cancellation wait for quiescence at all.

**TASK-29** remains responsible for the time-deadline path, but see the finding below: a
check-extension ply cap alone does **not** bound this window.

## Code-path reasoning

- `Search::run` resets `min_search_complete` to false, and `iterative_deepening` always starts at
  depth 1, so the window cannot widen with the requested depth.
- `Search::stopping` returns false while `min_search_complete` is false, suppressing both the atomic
  cancellation flag and `stop_time`. Every `stopping()` check inside `search`, `quiesce` and
  `quiesce_evasions` is inert until the full depth-1 iteration returns.
- A depth-1 child reaches depth zero and calls `quiesce`. The window is therefore the whole ply-1
  quiescence tree, not a depth-1 node count.
- Quiescence move selection has exactly two shapes. A q-node **not** in check expands queen
  promotions and captures only (`QMoveLoader`). A q-node **in** check is diverted to
  `quiesce_evasions` before the `OrderedMoves` loop and expands *every* legal move, quiet moves
  included. Note `QMoveLoader::load_quiets` is gated on `in_check()` but is unreachable from
  `quiesce` for this reason; in-check nodes never reach it.
- Consequently a quiet move enters the quiescence tree only as a check evasion. A *self-sustaining*
  quiet chain additionally requires each evasion to give check back, i.e. mutual alternating check.
- The only non-cap terminations are `quiesce` Step 1: threefold repetition and
  `half_move_clock() >= 50`.
- **This is why the fifty-move argument does not bound the tree.** Quiet check evasions raise the
  halfmove clock, but captures and pawn moves reset it. A tree that interleaves captures with check
  evasions never approaches either termination condition, and depth grows accordingly. Termination
  is guaranteed; a useful millisecond bound is not.

## Structural evidence

Structural measurement uses `engine/examples/task39_qtree.rs`, an offline reachability model. It
lives outside `engine/src` so that this ticket lands no change to search/stop/UCI-I/O production
code (criterion #1). It replicates quiescence *move selection* exactly as described above, and its
only terminations are `quiesce` Step 1 plus explicit ply/node caps.

It deliberately omits stand-pat, the TT cutoff and alpha-beta. Those only ever prune, so the model
is a sound **upper bound** on the ply-1 q-tree the engine can visit. A small figure from the model
is a real bound on the engine; a large figure bounds reachability only, and is not a claim that the
engine visits that many nodes.

Metrics: `max_q_ply` (deepest ply from the depth-1 child) and `max_quiet_check_chain` (longest run
of consecutive quiet check evasions — precisely the quantity a TASK-29 check-extension cap would
bound).

Reproduce with:

```
cargo run --release -p engine --example task39_qtree -- corpus 20000000
cargo run --release -p engine --example task39_qtree -- wac 2000000
cargo run --release -p engine --example task39_qtree -- sweep 5000 1580315493 200000
```

### Named corpus (20M node cap)

| Position | max_q_ply | quiet_chain | q_nodes | truncated |
| --- | ---: | ---: | ---: | --- |
| startpos | 1 | 0 | 20 | no |
| kiwipete_dense | 46 | 3 | 20,000,000 | yes |
| perft_checks_promotions | 46 | 4 | 20,000,000 | yes |
| dense_tactics | 40 | 2 | 20,000,000 | yes |
| many_captures | 48 | 3 | 20,000,000 | yes |
| capture_chain | 23 | 3 | 26,132,625 | yes |
| in_check_quiet_evasions | 1 | 0 | 4 | no |
| mate_tactics_1 | 38 | 4 | 20,000,000 | yes |
| mate_tactics_2 | 30 | 3 | 13,357,689 | no |
| check_heavy | 9 | 1 | 212 | no |
| adv_mutual_check_battery | 6 | 1 | 425 | no |
| adv_perpetual_check_queens | 6 | 1 | 42 | no |
| adv_discovered_check_battery | 5 | 1 | 96 | no |
| adv_rook_ladder_checks | 2 | 1 | 36 | no |
| adv_open_kings_many_evasions | 4 | 1 | 172 | no |
| adv_knight_check_net | 4 | 1 | 22 | no |

The six `adv_*` entries were constructed specifically to drive mutual quiet check chains
(check batteries, discovered-check batteries, perpetual-check material). All produced chains of
length 1 and trees under 500 nodes. **Hand-constructing a deep quiet check chain failed**, which is
itself the useful result: the mechanism is real but self-limiting, because each side must be in
check simultaneously and alternately.

The large trees come from ordinary dense tactical positions, and they come from capture/promotion
interleaving, not from quiet chains.

### Systematic search: Win At Chess suite, 300 positions (2M node cap)

201 of 300 positions exceeded the 2,000,000-node cap. Deepest reachable ply was 46.

Distribution of `max_quiet_check_chain`:

| chain length | positions |
| ---: | ---: |
| 0 | 2 |
| 1 | 24 |
| 2 | 38 |
| 3 | 216 |
| 4 | 20 |

Worst five by reachable depth:

| max_q_ply | quiet_chain | q_nodes | id | fen |
| ---: | ---: | ---: | --- | --- |
| 46 | 3 | 2,000,000+ | WAC.022 | `r1bqk2r/ppp1nppp/4p3/n5N1/2BPp3/P1P5/2P2PPP/R1BQK2R w KQkq - 0 1` |
| 45 | 3 | 2,000,000+ | WAC.263 | `rnbqr2k/pppp1Qpp/8/b2NN3/2B1n3/8/PPPP1PPP/R1B1K2R w KQ - 0 1` |
| 44 | 3 | 2,000,000+ | WAC.070 | `2kr3r/pppq1ppp/3p1n2/bQ2p3/1n1PP3/1PN1BN1P/1PP2PP1/2KR3R b - - 0 1` |
| 44 | 3 | 2,000,000+ | WAC.093 | `r1b1k1nr/pp3pQp/4pq2/3pn3/8/P1P5/2P2PPP/R1B1KBNR w KQkq - 0 1` |
| 44 | 3 | 2,000,000+ | WAC.114 | `r1b1rnk1/1p4pp/p1p2p2/3pN2n/3P1PPq/2NBPR1P/PPQ5/2R3K1 w - - 0 1` |

`WAC.104` (`b4r1k/pq2rp2/1p1bpn1p/3PN2n/2P2P2/P2B3K/1B2Q2N/3R2R1 w - - 0 1`) produced the longest
quiet check chain seen in the WAC suite, 4.

Because 201 positions truncated, their `q_nodes` and `max_q_ply` are lower bounds, and their chain
figures are lower bounds too. Raising the cap to 20M on the named corpus did not raise the maximum
chain above 4.

### Systematic search: 5,000 random positions (200k node cap, seed 1580315493)

Positions are generated by random legal play from the start position (4-63 plies), so this corpus
is uncurated and independent of the tactical bias of WAC.

Distribution of `max_quiet_check_chain`:

| chain length | positions |
| ---: | ---: |
| 0 | 120 |
| 1 | 176 |
| 2 | 1,443 |
| 3 | 3,186 |
| 4 | 73 |
| 5 | 2 |

Deepest reachable ply was 55, on ordinary early-middlegame positions such as
`rnbq1bnr/pppppkp1/8/5p1p/8/BP5P/P1PPPPP1/RN1QKBNR w KQ - 2 4`. Almost every position hit the
200,000-node cap, so both `q_nodes` and `max_q_ply` here are lower bounds.

### Summary of the adversarial search

Across all four corpora — 5,000 random positions, the 300-position WAC suite, the 16-position named
corpus, and six purpose-built check batteries — the longest consecutive quiet check-evasion chain
observed was **5**, and the great majority of positions sit at 2 or 3. Deliberate construction of a
long chain failed; the mechanism is real but self-limiting, because sustaining it requires both
sides to be in check alternately and repetition then cuts it.

Reachable tree *size and depth*, by contrast, is large almost everywhere: 46 ply on WAC, 55 ply in
the random sweep, and node counts past the caps on the majority of positions in both. The
adversarial case for this window is therefore not the exotic check chain the ticket anticipated —
it is the ordinary dense tactical position.

## Measured stop latency

Harness `tools/task39_stop_probe.rb`, extended with the six structurally worst positions found
above so that measured latency is tied to the adversarial corpus rather than only to hand-picked
positions.

- Build/base: see the handoff comment on TASK-39.
- Apple M3 Pro, Darwin arm64, `cargo build --release --bin seaborg`.
- One persistent UCI process, `Hash=1`, `uci`/`isready` warm-up.
- Per sample: `ucinewgame`, `position`, then `go infinite` immediately followed by `stop`.
- Duration starts before writing `go` and ends when `bestmove` is read, so it includes command
  dispatch, cancellation, the entire suppressed interval, formatting and pipe I/O.
- 1,000 samples per position, 16,000 total. Every sample returned a legal non-null bestmove.

| Position | median ms | p95 ms | max ms |
| --- | ---: | ---: | ---: |
| startpos | 0.068 | 0.107 | 0.594 |
| kiwipete_dense | 0.600 | 0.706 | 5.820 |
| perft_checks_promotions | 0.705 | 0.795 | 1.898 |
| dense_tactics | 0.082 | 0.116 | 0.536 |
| many_captures | 0.123 | 0.169 | 1.147 |
| capture_chain | 0.136 | 0.180 | 0.457 |
| in_check_quiet_evasions | 0.076 | 0.113 | 0.681 |
| mate_tactics_1 | 0.242 | 0.306 | 0.506 |
| mate_tactics_2 | 0.093 | 0.132 | 0.598 |
| check_heavy | 0.079 | 0.134 | 0.592 |
| model_worst_wac022 | 0.116 | 0.160 | 1.084 |
| model_worst_wac263 | 0.574 | 0.673 | 2.914 |
| model_worst_wac070 | 0.171 | 0.226 | 4.835 |
| model_worst_wac093 | 0.079 | 0.119 | 4.061 |
| model_worst_wac114 | 0.269 | 0.338 | 0.700 |
| model_worst_chain_wac104 | 1.162 | 1.355 | 4.250 |

Overall maximum 5.820 ms; every median at or below 1.162 ms.

**The gap is the finding.** `model_worst_wac114` has a reachable ply-1 q-tree of at least two
million nodes to 44 ply, and the engine answers `stop` on it in 0.269 ms median / 0.700 ms max. Six
to seven orders of magnitude of that reachable tree are removed by stand-pat, TT cutoffs and
alpha-beta. Responsiveness today rests entirely on pruning effectiveness, which is a heuristic
property with no worst-case guarantee, not on the suppression window being structurally short.

Quit was measured separately, since it terminates the process and cannot reuse a persistent engine:
50 warmed-handshake processes on Kiwipete sent `position`, `go infinite`, `quit`; elapsed from
sending through process exit was min 0.673 ms, median 0.887 ms, p95 1.247 ms, max 4.102 ms.

## Interaction with TASK-29

**A quiescence check-extension ply cap alone would not bound this window.** This corrects the
earlier reading of this question.

The observed `max_quiet_check_chain` never exceeded 5 across 5,000 swept positions, the 300-position
WAC suite, the named corpus and six purpose-built check batteries; most positions sit at 2 or 3. A
cap set at any plausible value (8, 16) would therefore almost never bind. Meanwhile the trees that are actually large — 46 ply,
tens of millions of reachable nodes — are driven by capture and promotion interleaving, which a
check-extension cap does not touch, and which resets the halfmove clock so that `quiesce` Step 1
never fires.

TASK-29 remains worth doing on its own merits (it bounds a genuinely unbounded recursion and helps
the time-deadline path), but it should not be recorded as the fix for stop responsiveness, and
closing it would not close this concern. Bounding the ply-1 window by structure alone would require
a total q-node or total q-ply budget, not a check-extension cap.

## Stop, quit, EOF and teardown

`engine/src/engine.rs` routes `stop` to `stop_search`, which calls `cancel` then `finish_search`;
`finish_search` blocks in `SearchHandle::wait`. Quit, stdin EOF and input errors use the same
`stop_search` call before breaking the driver loop. Replacement `go`, `setoption` and `ucinewgame`
also stop and join the active search first.

All of these therefore share the suppressed interval. Quit and EOF do not bypass or kill the worker:
process teardown waits for depth 1 plus its full quiescence, then emits `bestmove` and exits. The
separate quit measurements above confirm the same small observed delay plus teardown overhead. The
same unbounded-in-principle caveat applies to teardown as to `stop`.

TASK-34/TASK-37 coordination is intact: a change that simply re-enables cancellation before a legal
fallback exists would restore `bestmove 0000` on immediate stop/EOF. TASK-45 requires the legal
fallback first, and so preserves the EOF guarantee by construction.

## Judgement against runners

The 100 ms threshold is conservative against the measured sub-6 ms envelope and short enough to feel
prompt interactively. It also exceeds ordinary scheduling and pipe jitter, so a diagnostic check can
distinguish engine work from noise. It is not a safe clock-overrun allowance: at tc=2+0.05 the
increment is only 50 ms, and FastChess's configurable `timemargin` means a tournament may grant no
grace at all.

- Current typical behavior is acceptable in observed play, and agrees with TASK-32's zero
  time-forfeit self-play evidence.
- The uncapped theoretical deadline overrun is not acceptable as an invariant.
- Explicit UCI cancellation should not wait on quiescence at all once a legal fallback exists;
  TASK-45 addresses this directly and is the primary outcome of this investigation.
- No wall-clock regression test is added, because it would pass on this hardware and corpus while
  the adversarial failure mode remained open. TASK-45 requires deterministic cancellation coverage
  instead.
