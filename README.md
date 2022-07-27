# Seaborg

Seaborg is a chess engine written from scratch, as a project to learn
Rust. It isn't based on any existing engine, although the move
generation scheme is heavily inspired by the clever monomorphization 
approach used in the [Pleco](https://github.com/sfleischman105/Pleco) engine. 

## Past and future development

So far, Seaborg is quite primitive. He implements the UCI protocol, and
can sometimes be found playing on [Lichess](https://lichess.org/@/seaborg1) 
where he confounds opponents by playing bizarre opening plans like 1. ...a6, 
2. ...b6, 3. ...c6 etc. in every game. 

The reason for this strange style of play
is that Seaborg has no positional concepts built in (yet!) - the
evaluation function is currently purely material based, and so with no
ability to differentiate between moves so early in the game (when
material remains balanced in almost every variation to the horizon),
every move looks equally good to Seaborg, so he selects the first one he
sees..! Later in the game, Seaborg is often able to crush weaker
opponents tactically, even after emerging from the opening with a
horrible position.

In the initial development phase, I wanted to build a solid internal board
representation, fast move generation, a variety of standard search features,
including transposition tables, as well as the UCI protocol. All of this
provides a base to continue developing the engine and start adding more
positional awareness.

I hope to have more time in the future to work on improving the search
algorithm, the evaluation function, add multithreading and perhaps even
experiment with some neural net implementations for static evaluation.

## Features

* Engine
  * [Bitboard](https://www.chessprogramming.org/Bitboards) board representing
  * [Magic bitboard](https://www.chessprogramming.org/Magic_Bitboards) move generator
  * [Pleco](https://github.com/sfleischman105/Pleco)-inspired move 
    generation scheme, using generics and traits to expand several 
    movegen functions at compile time. This approach increases code size
    in the compiled binary, but keeps the source code very clean and
    readable, while removing almost all branching from the movegen
    algorithm to keep it as fast as possible. The approach can probably
    be taken even further in future versions
  * [Transposition table](https://www.chessprogramming.org/Transposition_Table)
  * [UCI protocol](https://www.chessprogramming.org/UCI)
* Search
  * [Alpha-beta search](https://www.chessprogramming.org/Alpha-Beta)
  * [Quiescence search](https://www.chessprogramming.org/Quiescence_Search)
  * [Iterative deepening](https://www.chessprogramming.org/Iterative_Deepening)
  * [Move ordering](https://www.chessprogramming.org/Move_Ordering)
    * [PV-move](https://www.chessprogramming.org/PV-Move)
    * [MVV-LVA](https://www.chessprogramming.org/MVV-LVA)
    * [Internal iterative deepening](https://www.chessprogramming.org/Internal_Iterative_Deepening)
  * Basic [time management](https://www.chessprogramming.org/Time_Management)
* Evaluation
  * [Material](https://www.chessprogramming.org/Material) counting

## Future features

In future, I would like to implement:

* [Piece-square tables](https://www.chessprogramming.org/Piece-Square_Tables)
* [Killer move heuristic](https://www.chessprogramming.org/Killer_Heuristic)
* [Multithreaded search](https://www.chessprogramming.org/Parallel_Search)
* [Pawn hash tables](https://www.chessprogramming.org/Pawn_Hash_Table)
* [Material hash table](https://www.chessprogramming.org/Material_Hash_Table) 
* Maybe some neural net approach for evaluation and/or move ordering
* Probably lots of other stuff to be tried..!
