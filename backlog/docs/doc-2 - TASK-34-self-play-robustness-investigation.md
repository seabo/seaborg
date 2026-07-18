---
id: doc-2
title: TASK-34 self-play robustness investigation
type: specification
created_date: '2026-07-18 01:19'
updated_date: '2026-07-18 01:22'
---
# TASK-34 — Self-play robustness investigation findings

Investigation only. **No engine code changed under TASK-34.** Deliverable: root-cause
evidence for the three failure modes plus fresh implementation tickets.

Environment: master @ `d9a138c`, macOS 15.6.1 (arm64), FastChess (`~/.local/bin/fastchess`),
release and debug builds of `target/*/seaborg -u`, `python-chess` 1.11.2 for PV validation.

---

## Defect 3 — Search aborts to the null move on stdin EOF

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
- Defect 3 → **TASK-37** EOF legal-move fix (legal best-so-far on stdin EOF; **depends on
  and coupled to TASK-32**; regression coverage of the stop/abort and EOF paths). The
  shared "legal move before any abort" guarantee is recorded on both TASK-37 and TASK-32
  so the fix is implemented once.
