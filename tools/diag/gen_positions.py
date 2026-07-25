#!/usr/bin/env python3
"""Generate a diverse position set for the eval-agreement test (TASK-82, AC#3).

Plays lightly-randomised games and samples one quiet position from each, so the
resulting set spans game phases and material balances the way real play does
rather than clustering on tactics. Moves are Stockfish's shallow choice most of
the time with an epsilon fraction of random legal moves for diversity; the
sampled position is required to be not in check and to have legal replies.

Needs python-chess (board legality and FEN) and a Stockfish binary. On a host
with `uv` this runs without a manual install:

    uv run --with chess python3 gen_positions.py \
        --stockfish PATH --games 400 --out positions.fen

The output is one FEN per line, consumed by eval_agreement.py.
"""

from __future__ import annotations

import argparse
import random

import chess
import chess.engine


def sample_game(engine, rng, epsilon, min_ply, max_ply):
    board = chess.Board()
    target = rng.randint(min_ply, max_ply)
    sampled = None
    for ply in range(max_ply):
        if board.is_game_over(claim_draw=False):
            break
        legal = list(board.legal_moves)
        if not legal:
            break
        if rng.random() < epsilon:
            move = rng.choice(legal)
        else:
            move = engine.play(board, chess.engine.Limit(depth=6)).move or rng.choice(legal)
        board.push(move)
        # Sample once we are at the target ply and the position is quiet enough
        # to be a fair static-eval subject: side to move not in check, and not a
        # terminal position.
        if ply + 1 >= target and not board.is_check() and board.legal_moves:
            sampled = board.fen()
            break
    return sampled


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stockfish", required=True)
    ap.add_argument("--games", type=int, default=400)
    ap.add_argument("--epsilon", type=float, default=0.2)
    ap.add_argument("--min-ply", type=int, default=12)
    ap.add_argument("--max-ply", type=int, default=70)
    ap.add_argument("--seed", type=int, default=82)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    rng = random.Random(args.seed)
    engine = chess.engine.SimpleEngine.popen_uci(args.stockfish)
    engine.configure({"Threads": 1, "Hash": 64})

    seen: set[str] = set()
    fens: list[str] = []
    try:
        while len(fens) < args.games:
            fen = sample_game(engine, rng, args.epsilon, args.min_ply, args.max_ply)
            if fen and fen not in seen:
                seen.add(fen)
                fens.append(fen)
    finally:
        engine.quit()

    with open(args.out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(fens) + "\n")
    print(f"wrote {len(fens)} positions to {args.out}")


if __name__ == "__main__":
    main()
