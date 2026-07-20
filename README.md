# Seaborg

Seaborg is a chess engine written from scratch in Rust. It isn't based
on any existing engine, although the move generation scheme is heavily
inspired by the approach used in [Pleco](https://github.com/sfleischman105/Pleco).

## Building

`cargo build` is all that is required. The build embeds the current Git commit,
reported in the startup banner that `seaborg --uci` writes to the diagnostic
channel. UCI traffic on stdout carries only the name and version.

Git is optional. Building from a source archive, or on a machine without Git
installed, succeeds and embeds the commit as `unknown`. To pin a known revision
in that situation — for a release tarball or a distribution package — set
`SEABORG_GIT_HASH` at build time:

```sh
SEABORG_GIT_HASH=$(cat REVISION) cargo build --release
```

## UCI options

Seaborg advertises exactly the options it implements:

- `Hash` (spin, default 16, min 1, max 1024) — size in MiB of the transposition
  table. A change takes effect only at a quiescent boundary: any running search
  is stopped and joined before the table is reallocated, so a resize never pulls
  the allocation out from under a live search.

`Threads` is not advertised. The search currently runs a single worker, so there
is no worker count to configure; a `Threads` option appears once Lazy SMP
multithreading lands. For forward compatibility seaborg tolerates an unrecognised
`setoption` — it is reported on the diagnostic channel and otherwise ignored — so
a harness that always sends `Threads` still runs against today's build.

## Past and future development

Seaborg currently has minimal built-in understanding of chess strategy -
the evaluation function is simple material counting. I've been reluctant
to spend time working on something more complex than that as I'd like to
incorporate a neural net-based approach in the future.

Seaborg implements the UCI protocol, and can sometimes be found playing
on [Lichess](https://lichess.org/@/seaborg1). He usually confounds
opponents by playing bizarre opening plans like 1. ...a6, 2. ...b6, 3. ...c6 etc.
in every game.

With no ability to differentiate between moves so early in the game (when
material remains balanced in almost every variation to the horizon),
every move looks equally good to Seaborg, so he selects the first one he
sees..! Later in the game, Seaborg is often able to crush weaker
opponents tactically, even after emerging from the opening with a
horrible position.

Repository-owned, reproducible engine strength comparisons are documented in
[docs/strength-testing.md](docs/strength-testing.md).

During the initial development, I wanted to build a solid internal board
representation, fast move generation, a variety of standard search features,
including transposition tables, as well as the UCI protocol. All of this
provides a base to continue developing the engine and start adding more
positional awareness.

## Features

- Engine
  - [Bitboard](https://www.chessprogramming.org/Bitboards) board representing
  - [Magic bitboard](https://www.chessprogramming.org/Magic_Bitboards) move generator
  - [Pleco](https://github.com/sfleischman105/Pleco)-inspired move
    generation scheme, using generics and traits. This approach increases code size
    in the compiled binary, but keeps the source code very clean and
    readable, while removing almost all branching from the movegen
    algorithm.
  - [Lockless shared transposition table](https://www.chessprogramming.org/Transposition_Table)
  - [UCI protocol](https://www.chessprogramming.org/UCI)
- Search
  - [Alpha-beta search](https://www.chessprogramming.org/Alpha-Beta)
  - [Quiescence search](https://www.chessprogramming.org/Quiescence_Search)
  - [Iterative deepening](https://www.chessprogramming.org/Iterative_Deepening)
  - [Move ordering](https://www.chessprogramming.org/Move_Ordering)
    - [Static exchange evaluation](https://www.chessprogramming.org/Static_Exchange_Evaluation)
    - [PV-move](https://www.chessprogramming.org/PV-Move)
    - [MVV-LVA](https://www.chessprogramming.org/MVV-LVA)
  - [Killer move heuristic](https://www.chessprogramming.org/Killer_Heuristic)
  - Basic [time management](https://www.chessprogramming.org/Time_Management)
- Evaluation
  - [Material](https://www.chessprogramming.org/Material) counting

## Future features

The main future development direction is to improve static evaluation at
leaf nodes using a neural net approach.

- [Efficiently-updatable neural network](https://www.chessprogramming.org/NNUE)
- [Lazy SMP multithreading](https://www.chessprogramming.org/Lazy_SMP) — the
  search runs a single worker today; the configuration already carries a worker
  count so a multi-worker search can be enabled without reworking option
  ownership.

## Development

Run these three commands before proposing a change:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

CI runs exactly these, so a change that fails any of them locally will fail
there too. Clippy is a gate rather than advice — `-D warnings` turns any
warning into a build failure. Fix warnings at the source; reach for a local
`#[allow]` only where the warned construct is genuinely required, and say why
in a comment.

Tests run in the debug profile. The engine leans on `debug_assert!` to catch
invalid board states, and those assertions disappear from a release build, so
`--release` is not a substitute.

CI pins the Rust toolchain in `.github/workflows/ci.yml`. Local runs use
whatever toolchain you have installed, so an unusually old or new one can
disagree with CI about formatting or lints; the pinned version there is the
reference.
