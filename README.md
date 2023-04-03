# Seaborg

Seaborg is a chess engine written from scratch in Rust. It isn't based
on any existing engine, although the move generation scheme is heavily
inspired by the approach used in [Pleco](https://github.com/sfleischman105/Pleco).

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
  - [LazySMP multithreading](https://www.chessprogramming.org/Lazy_SMP)
  - Basic [time management](https://www.chessprogramming.org/Time_Management)
- Evaluation
  - [Material](https://www.chessprogramming.org/Material) counting

## Future features

The main future development direction is to improve static evaluation at
leaf nodes using a neural net approach.

- [Efficiently-updatable neural network](https://www.chessprogramming.org/NNUE)
