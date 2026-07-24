#!/usr/bin/env python3
"""Search-free static-eval agreement test (TASK-82, AC#3).

Isolates evaluation quality from search. For each position in a reference set:

* a reference label is produced by a deep Stockfish search (the ground truth);
* each engine's STATIC evaluation (no search) is queried:
  - seaborg via the `eval` command (`staticeval cp <v>`, side-to-move view);
  - Stockfish via its `eval` command (`Final evaluation ... (white side)`).

All scores are normalised to White's perspective, then compared to the
reference on scale-independent metrics — never raw centipawns, since the two
engines' cp scales differ:

* Spearman rank correlation between each engine's static eval and the deep label;
* winner-prediction accuracy on decisive positions (sign agreement with the
  label where |label| exceeds a margin).

Usage:
    python3 eval_agreement.py --seaborg PATH --stockfish PATH \
        --positions positions.fen --label-depth 20 --out eval_agreement.json
"""

from __future__ import annotations

import argparse
import json
import re

from uci import Engine

MATE_CP = 3000  # a forced mate is ranked beyond any material score


def side_to_move_is_white(fen: str) -> bool:
    return fen.split()[1] == "w"


def to_white_pov(cp_stm: int, fen: str) -> int:
    """Convert a side-to-move-relative score to White's perspective."""
    return cp_stm if side_to_move_is_white(fen) else -cp_stm


def deep_label(sf: Engine, fen: str, depth: int) -> int:
    """Deep Stockfish search score for `fen`, in White's perspective (cp)."""
    lines = sf.command(f"position fen {fen}\ngo depth {depth}", "bestmove")
    score_stm = None
    for line in lines:
        if not line.startswith("info"):
            continue
        m = re.search(r"score (cp|mate) (-?\d+)", line)
        if m:
            if m.group(1) == "mate":
                mate = int(m.group(2))
                score_stm = MATE_CP if mate > 0 else -MATE_CP
            else:
                score_stm = int(m.group(2))
    if score_stm is None:
        raise RuntimeError(f"no score parsed for {fen}")
    return to_white_pov(score_stm, fen)


def seaborg_static(engine: Engine, fen: str) -> int:
    lines = engine.command(f"position fen {fen}\neval", "staticeval")
    for line in lines:
        m = re.match(r"staticeval cp (-?\d+)", line)
        if m:
            return to_white_pov(int(m.group(1)), fen)
    raise RuntimeError(f"seaborg gave no static eval for {fen}")


def stockfish_static(sf: Engine, fen: str) -> int | None:
    """Stockfish static eval in White's perspective (cp), or None if it refuses
    (it declines to statically evaluate a position with the side to move in
    check)."""
    lines = sf.command(f"position fen {fen}\neval", "Final evaluation")
    for line in lines:
        if line.startswith("Final evaluation"):
            if "none" in line.lower():
                return None
            m = re.search(r"(-?\d+\.\d+)", line)
            if m:
                return int(round(float(m.group(1)) * 100.0))  # already White pov
    return None


def rank(values: list[float]) -> list[float]:
    """Average ranks, so ties do not bias the correlation."""
    order = sorted(range(len(values)), key=lambda i: values[i])
    ranks = [0.0] * len(values)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and values[order[j + 1]] == values[order[i]]:
            j += 1
        avg = (i + j) / 2.0 + 1.0
        for k in range(i, j + 1):
            ranks[order[k]] = avg
        i = j + 1
    return ranks


def pearson(a: list[float], b: list[float]) -> float:
    n = len(a)
    ma, mb = sum(a) / n, sum(b) / n
    cov = sum((x - ma) * (y - mb) for x, y in zip(a, b))
    va = sum((x - ma) ** 2 for x in a) ** 0.5
    vb = sum((y - mb) ** 2 for y in b) ** 0.5
    return cov / (va * vb) if va and vb else 0.0


def spearman(a: list[float], b: list[float]) -> float:
    return pearson(rank(a), rank(b))


def winner_accuracy(static: list[float], label: list[float], margin: int):
    hit = total = 0
    for s, l in zip(static, label):
        if abs(l) < margin:
            continue
        total += 1
        if (s > 0) == (l > 0):
            hit += 1
    return hit, total


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seaborg", required=True)
    ap.add_argument("--stockfish", required=True)
    ap.add_argument("--positions", required=True)
    ap.add_argument("--label-depth", type=int, default=20)
    ap.add_argument("--margin", type=int, default=100)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    with open(args.positions, encoding="utf-8") as fh:
        fens = [ln.strip() for ln in fh if ln.strip() and not ln.startswith("#")]

    seaborg = Engine([args.seaborg], {"Threads": "1", "Hash": "64"})
    sf = Engine([args.stockfish], {"Threads": "1", "Hash": "64"})

    labels, sb_static, sf_static, used = [], [], [], []
    try:
        for i, fen in enumerate(fens):
            sf_s = stockfish_static(sf, fen)
            if sf_s is None:
                continue  # position Stockfish will not statically evaluate
            label = deep_label(sf, fen, args.label_depth)
            sb_s = seaborg_static(seaborg, fen)
            labels.append(label)
            sb_static.append(sb_s)
            sf_static.append(sf_s)
            used.append(fen)
            if (i + 1) % 50 == 0:
                print(f"  labelled {i + 1}/{len(fens)}")
    finally:
        seaborg.quit()
        sf.quit()

    sb_rho = spearman(sb_static, labels)
    sf_rho = spearman(sf_static, labels)
    sb_hit, tot = winner_accuracy(sb_static, labels, args.margin)
    sf_hit, _ = winner_accuracy(sf_static, labels, args.margin)

    print(f"\npositions used: {len(used)} (of {len(fens)}), "
          f"decisive (|label|>{args.margin}cp): {tot}")
    print(f"  seaborg  static vs deep label : Spearman rho = {sb_rho:.4f}, "
          f"winner acc = {sb_hit}/{tot} = {sb_hit / tot:.3f}")
    print(f"  stockfish static vs deep label : Spearman rho = {sf_rho:.4f}, "
          f"winner acc = {sf_hit}/{tot} = {sf_hit / tot:.3f}")

    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            json.dump(
                {
                    "label_depth": args.label_depth,
                    "margin": args.margin,
                    "n_positions": len(used),
                    "n_decisive": tot,
                    "seaborg_spearman": sb_rho,
                    "stockfish_spearman": sf_rho,
                    "seaborg_winner_acc": sb_hit / tot if tot else None,
                    "stockfish_winner_acc": sf_hit / tot if tot else None,
                    "rows": [
                        {"fen": f, "label": l, "seaborg": a, "stockfish": b}
                        for f, l, a, b in zip(used, labels, sb_static, sf_static)
                    ],
                },
                fh,
                indent=2,
            )
        print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
