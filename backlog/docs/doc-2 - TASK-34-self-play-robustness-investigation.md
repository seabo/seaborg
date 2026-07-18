---
id: doc-2
title: TASK-34 self-play robustness investigation
type: specification
created_date: '2026-07-18 01:19'
updated_date: '2026-07-18 12:04'
---
# TASK-34 — Self-play robustness investigation findings

Investigation only. **No engine code changed under TASK-34.** Deliverable: root-cause
evidence for the three failure modes plus fresh implementation tickets.

> **Status update (2026-07-18, TASK-34 rework).** Defect 3 (EOF null move) **no longer
> reproduces**: it was fixed as a side effect of TASK-32, merged at `8ceb480` after this
> investigation was first written. The Defect 3 section below records the original
> root-cause analysis against master `d9a138c` and remains accurate for that commit; see
> [Defect 3 re-verification](#defect-3--re-verification-against-merged-task-32) for the
> current state. Defects 1 and 2 are unaffected and still open.

Environment: master @ `d9a138c`, macOS 15.6.1 (arm64), FastChess (`~/.local/bin/fastchess`),
release and debug builds of `target/*/seaborg -u`, `python-chess` 1.11.2 for PV validation.

---

## Defect 3 — Search aborts to the null move on stdin EOF (FIXED by TASK-32)

*Analysis below is against master `d9a138c`. Superseded on current master — see the
re-verification section.*

**Reproduction (deterministic):**

```
printf 'uci\nisready\ngo depth 25\n' | target/release/seaborg -u
...
readyok
bestmove 0000
```

The startpos has 20 legal moves, yet the engine emits `bestmove 0000` when stdin
closes mid-search.

**Root cause (confirmed):**

1. `read_commands` reads the three lines, then `read_line` returns `Ok(0)` (EOF) and
   sends `Input::Closed` (`engine/src/engine.rs:120-124`).
2. The driver handles `Input::Closed` by cancelling the running search and finishing it
   (`engine/src/engine.rs:53-60` → `stop_search` → `cancel()` + `finish_search`).
3. `CancellationToken::cancel` sets the shared flag; the search sees `stopping()` and
   unwinds returning `Score::zero()` (`search.rs:480-482`, `619-621`).
4. `iterative_deepening` only records a `SearchResult` for an iteration that completes
   while **not** stopping (`search.rs:447-457`). If the cancel lands before even depth 1
   completes, `result` stays `None` and the outcome is `SearchOutcome::Cancelled(None)`.
5. `format_search_outcome` maps a `None` best move to the literal `"bestmove 0000"`
   (`engine/src/info.rs:34-38`).

So on EOF the engine cancels before guaranteeing a completed root move, and reports a
null move even though legal moves exist.

**Relevant code:** `engine/src/engine.rs` (EOF/Closed branch), `engine/src/search.rs`
(`iterative_deepening`, cancellation checks), `engine/src/info.rs`
(`format_search_outcome`).

---

## Defect 2 — Illegal moves in the reported principal variation

**Reproduction (authoritative, FastChess):** seaborg-vs-seaborg, `depth=4`, 40 games:

```
Warning; Illegal PV move - move c5f8 from A
Info; info depth 4 multipv 1 score mate -2 nodes 2794 nps 3027085 hashfull 9 time 0 pv d7f8 g6a6 f8g6 c5f8
```

FastChess plays the games to correct conclusions ("White mates"), so **the reported
best move (first PV ply) is legal** — this is a PV-*reporting* defect, not a move-selection
defect. The corruption is on the deeper PV plies (here the 4th ply `c5f8`), and it shows
up on mate lines (`score mate -2`).

Programmatic validation of the *final completed* depth (depth 8) across a dozen tactical
positions found no illegal PV, i.e. the defect surfaces on shallow/partial and mate-scored
lines emitted during play rather than on every line.

**Root cause (characterised):** the PV shown over UCI is rebuilt from the triangular
`PVTable` (`engine/src/pv_table.rs`) via `emit_progress` → `pvt.pv()` (`search.rs:931-941`).
The table is updated on every alpha-raise, **including fail-high / beta-cutoff nodes**:
`search.rs:671-698` calls `self.pvt.copy_to(depth, *mov)` in the `else` (value ≥ beta)
branch before `break 'move_loop`. `copy_to`/`update_internal` (`pv_table.rs:36-69`)
splices the child row into the parent row via `copy_within`, but on a cutoff (and around
mate/leaf handling via `pv_leaf_at`, `pv_table.rs:45-50`) the child row can still hold
moves left over from a **different sibling subtree**. Those stale moves get copied up, so
the reconstructed PV is a sequence that does not chain legally beyond the first move.
Mate/stalemate leaf handling (`search.rs:562-565`, `712-721`) interacts with this because
the mate PV is truncated/leaf-filled rather than being a validated line.

**Relevant code:** `engine/src/search.rs` (Step 22 PV update on cutoffs; mate/leaf
handling), `engine/src/pv_table.rs` (`copy_to`/`update_internal`/`pv_leaf_at`),
`engine/src/info.rs` (`format_search_event` PV formatting).

---

## Defect 1 — Intermittent search/UCI completion deadlock

**Reproduction (intermittent, reproduced):** seaborg-vs-seaborg fixed depth under load.
The **debug build** (which is ~15–20× slower and shifts thread timing) hangs readily; a
release run of 400 games at depth 6 did not hang in one sitting, matching the reported
nondeterminism.

- `depth=5`, `concurrency=8`: after ~48–72 completed games, all 8 concurrent slots freeze
  with games "Started" but never "Finished" (e.g. games 49–56, then 73–80 in separate
  runs). Engines go to near-zero CPU and never emit `bestmove`. Orphaned engine processes
  from a killed run remained alive and stuck for minutes afterwards.
- **No panic** appears in engine stdout/stderr or the FastChess trace log — this is a
  genuine deadlock, not a crash.

**Root cause (confirmed via thread samples at the hang):**

Sampling the stuck engines with `sample <pid>`:

- Hung searcher (e.g. pid 24415): the **driver/main thread is parked inside
  `crossbeam_channel::select!`** on the active-search branch of `next_event`
  (`engine/src/engine.rs:144-153`) — i.e. `active_search.is_some()`. Only **two** threads
  exist: main and the stdin reader. **The search worker thread is gone** (it finished and
  became an unjoined zombie), which means it already dropped its `SearchEvent` `Sender`.
- Its partner (e.g. pid 24417): main thread parked in `commands.recv()` (no active search)
  and the reader thread blocked in `read_line` — a normal victim waiting for its opponent's
  move that never arrives.

The driver detects normal search completion **only** via the events channel becoming
disconnected: the worker's `Sender` is moved into the search thread (`search.rs:150-165`),
and its drop on thread exit is expected to make `recv(search.events())` return
`Err(Disconnected)` → `DriverEvent::Search(Err(_))` → `finish_search` emits `bestmove`
(`engine/src/engine.rs:105-110`, `223-230`). At the hang, the worker **has** exited and
dropped the sender, but the parked `select!` never observed the disconnection, so the
driver waits forever for a search event that can never come and `bestmove` is never
emitted.

In short: **the search-completion signal relies on a dropped-`Sender` channel disconnect
waking a parked `select!`, and that wakeup is being lost**, deadlocking the driver. The
dependency is `crossbeam-channel v0.5.6`. Candidate fixes to spec: send an explicit
terminal "search done" message before the worker returns (do not rely on disconnect),
and/or make `next_event` robust to a finished handle (e.g. consult
`SearchHandle::is_finished()` / join on a completion notification), and/or upgrade
crossbeam-channel; the fix must be validated by a stress harness and a targeted
completion-race regression test.

**Relevant code:** `engine/src/engine.rs` (`run` loop, `next_event` `select!`,
`finish_search`), `engine/src/search.rs` (`SearchEngine::start` worker + `Sender`,
`SearchHandle`), `Cargo`/`crossbeam-channel 0.5.6`.

---

## Defect 3 — re-verification against merged TASK-32

Re-run on the TASK-34 branch with master merged in (release build, commit `d6c5679`,
which contains TASK-32's `8ceb480`). Every EOF variant now returns a legal move:

| Input (piped, stdin closes at end) | Before (`d9a138c`) | After (merged TASK-32) |
| --- | --- | --- |
| `uci/isready/go depth 25` (doc's original repro) | `bestmove 0000` | `bestmove a2a3` |
| `uci/isready/position startpos/go depth 8` | `bestmove 0000` | `bestmove a2a3` |
| `uci/isready/position fen <Kiwipete>/go depth 20` | — | `bestmove e2a6` |
| `uci/isready/position startpos/go infinite` | — | `bestmove a2a3` |
| `uci/isready/position startpos/go depth 25/quit` | — | `bestmove a2a3` |

The post-ply-1 half of the path was checked separately: holding stdin open for ~3s during
`go infinite` and then closing it returned the **depth-10** result (`bestmove a2a3`), i.e.
an abort after ply 1 yields `Cancelled(Some(result))` and `SearchOutcome::result()` returns
the last completed iteration's move rather than `None`. An explicit `stop` after 3s behaves
identically.

Terminal positions are unchanged and still correct: `7k/5QQ1/8/8/8/8/8/7K b` (checkmate) and
`7k/5Q2/6K1/8/8/8/8/8 b` (stalemate) both emit `bestmove 0000`, which is conventional UCI.

**Mechanism.** TASK-32 added `Search::min_search_complete` (`engine/src/search.rs:366`).
`stopping()` (`search.rs:763`) returns `false` while that flag is unset, suppressing **both**
the time deadline and the cancellation flag; `iterative_deepening` arms it only after the
first iteration completes (`search.rs:468`). Step 4 of the original root-cause chain above —
"if the cancel lands before even depth 1 completes, `result` stays `None`" — can therefore no
longer occur. This works for EOF specifically because EOF reaches the search through the
*cancellation flag*, the same path TASK-32 suppresses: `Input::Closed` (`engine.rs:90`) →
`stop_search` → `cancel()`. TASK-32 suppressed the cancellation flag (not just the time
deadline) deliberately, precisely so an immediate `stop` during ply 1 could not produce
`bestmove 0000`.

**Consequence for this investigation.** Defect 3 needs no fix. The prediction recorded below
in "Independence and coupling" — that a single guarantee resolves both TASK-32 and Defect 3 —
held exactly, and TASK-32's implementation of it was sufficient on its own. What remains is
regression coverage: TASK-32's unit tests pin the search-level abort paths, but nothing
exercises the driver-level EOF path end to end and nothing pins the terminal-position case.
TASK-37 was narrowed to that tests-only scope rather than retired, so the fix-level
requirement (legal best-so-far on stdin EOF, with regression coverage) is not dropped.

**Coordination with TASK-39.** TASK-39 (filed on master) asks whether this same suppressed
window makes UCI `stop` too slow. It and TASK-37 examine the window from opposite directions:
TASK-37 depends on the window existing, TASK-39 asks whether it is too wide. Any narrowing of
the window must preserve the EOF guarantee, since `stop` and EOF share the cancellation flag.

---

## Independence and coupling with TASK-32

- **Defect 1 (completion deadlock)** and **Defect 2 (illegal PV)** are **independent** of
  each other, of Defect 3, and of TASK-32. Defect 1 is a concurrency/signalling bug in the
  driver; Defect 2 is a PV-table reconstruction correctness bug. Neither touches time
  allocation.
- **Defect 3 (EOF null move) shares a root cause with TASK-32** (time-abort null move):
  both are the *same* underlying defect — the search does not guarantee a chosen legal
  root move before an abort takes effect, so `SearchResult`/best move is `None` and UCI
  emits `bestmove 0000`. They differ only in the **abort trigger**: TASK-32's trigger is a
  zero/near-zero time budget; Defect 3's trigger is EOF/cancel. A fix that guarantees at
  least one completed root move (and returns the legal best-so-far on any abort) resolves
  both. To avoid duplicated fixes, the Defect-3 ticket must be coordinated with TASK-32:
  the shared "always choose a legal move before any abort" guarantee should be implemented
  once, with the EOF path added as an explicit trigger and regression case.

## Fresh tickets created

- Defect 1 → **TASK-35** completion-deadlock fix (no hang under repeated self-play;
  regression coverage of the completion/stop signalling).
- Defect 2 → **TASK-36** illegal-PV fix (only-legal PV moves; PV-legality regression).
- Defect 3 → **TASK-37**, **narrowed to regression coverage only** after re-verification
  showed TASK-32 already implements the shared "legal move before any abort" guarantee.
  It now covers the driver-level EOF path and the terminal-position case as tests, with no
  engine behaviour change. Priority dropped high → medium. The coupling and its resolution
  are recorded on TASK-32, TASK-37 and TASK-39.

Ordinals were reassigned (TASK-35 → 40000, TASK-36 → 41000, TASK-37 → 42000) to clear a
collision with TASK-38 and TASK-39, filed on master while TASK-34 was in review.
