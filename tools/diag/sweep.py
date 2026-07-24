#!/usr/bin/env python3
"""Handicap sweep to find the seaborg-vs-Stockfish parity point (TASK-82, AC#4).

At equal budgets Stockfish wins ~100% of games, so the raw gap is only a lower
bound ("more than ~700 Elo"). To turn the gap into a number, hold seaborg's
budget fixed and weaken Stockfish (fewer nodes, or less time) across a ladder,
locating the Stockfish budget at which the match is even. That parity budget is
the informative quantity:

* at fixed NODES, the parity node ratio (seaborg nodes / Stockfish nodes) is how
  many more nodes seaborg needs per Stockfish node — a combined eval +
  selectivity-per-node deficit, with the raw-speed axis removed;
* at fixed TIME, the parity time ratio folds NPS back in. Dividing the time
  ratio by the node ratio recovers the NPS ratio, cross-checking nps_ebf.py.

Usage:
    python3 sweep.py --seaborg SB --stockfish SF \
        --seaborg-limit nodes=200000 \
        --sf-limits nodes=200000,nodes=40000,nodes=10000,nodes=3000,nodes=1000 \
        --games 100 --concurrency 6 --openings ../strength/openings-v1.epd \
        --out sweep_nodes.json
"""

from __future__ import annotations

import argparse
import json
import subprocess
from argparse import Namespace

from gauntlet import build_cmd, parse_result


def run_one(base: Namespace, sf_limit: str):
    args = Namespace(**vars(base))
    args.sf_limit = sf_limit
    args.pgnout = None
    args.out = None
    out = subprocess.run(build_cmd(args), capture_output=True, text=True)
    return parse_result(out.stdout + out.stderr)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fastchess", default="fastchess")
    ap.add_argument("--seaborg", required=True)
    ap.add_argument("--stockfish", required=True)
    ap.add_argument("--seaborg-limit", required=True)
    ap.add_argument("--sf-limits", required=True,
                    help="comma-separated fastchess limit tokens for Stockfish")
    ap.add_argument("--games", type=int, default=100)
    ap.add_argument("--concurrency", type=int, default=6)
    ap.add_argument("--hash", type=int, default=64)
    ap.add_argument("--openings", required=True)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    base = Namespace(
        fastchess=args.fastchess, seaborg=args.seaborg, stockfish=args.stockfish,
        seaborg_limit=args.seaborg_limit, games=args.games,
        concurrency=args.concurrency, hash=args.hash, openings=args.openings,
    )

    print(f"seaborg fixed at: {args.seaborg_limit}\n")
    print(f"{'SF limit':<20} {'score%':>8} {'W-L-D':>14} {'Elo':>12}  Ptnml")
    rows = []
    for sf_limit in args.sf_limits.split(","):
        elo, err, wld, pct, penta = run_one(base, sf_limit)
        elo_s = "n/a" if elo is None else f"{elo:+.0f}+/-{err:.0f}"
        wld_s = "-".join(str(x) for x in wld) if wld else "?"
        print(f"{sf_limit:<20} {pct if pct is not None else -1:>8.2f} "
              f"{wld_s:>14} {elo_s:>12}  {penta}")
        rows.append({"sf_limit": sf_limit, "score_pct": pct, "wld": wld,
                     "elo": None if elo is None else (elo if abs(elo) != float("inf") else str(elo)),
                     "pentanomial": penta})

    # Report the bracket straddling 50%.
    ordered = [r for r in rows if r["score_pct"] is not None]
    below = [r for r in ordered if r["score_pct"] < 50]
    above = [r for r in ordered if r["score_pct"] >= 50]
    if below and above:
        lo = max(below, key=lambda r: r["score_pct"])
        hi = min(above, key=lambda r: r["score_pct"])
        print(f"\nparity (50%) is bracketed between SF {lo['sf_limit']} "
              f"({lo['score_pct']:.1f}%) and SF {hi['sf_limit']} ({hi['score_pct']:.1f}%)")
    else:
        print("\n50% not bracketed by this ladder; widen the SF handicap range")

    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            json.dump({"seaborg_limit": args.seaborg_limit, "rows": rows}, fh, indent=2)
        print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
