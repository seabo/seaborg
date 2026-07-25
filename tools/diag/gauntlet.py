#!/usr/bin/env python3
"""Head-to-head seaborg-vs-Stockfish gauntlet driver (TASK-82, AC#4).

Runs one fastchess match between seaborg and Stockfish with an independent
search limit per side, and parses the Elo difference and W-L-D. Independent
per-side limits are the point: a fixed-NODES match with equal node budgets
removes the raw-speed (NPS) axis, so the residual gap is eval + selectivity per
node; a fixed-TIME match adds NPS back. Handicapping Stockfish (fewer nodes or
less time) locates the budget at which the match is even, which quantifies how
far behind per node / per second seaborg is.

Limits are fastchess tokens, e.g. `nodes=100000`, `tc=10+0.1`, `st=0.1`,
`depth=12`. Both sides run single-threaded with a small hash.

Usage:
    python3 gauntlet.py --seaborg SB --stockfish SF \
        --seaborg-limit nodes=200000 --sf-limit nodes=50000 \
        --games 400 --concurrency 10 --openings openings.epd
"""

from __future__ import annotations

import argparse
import re
import subprocess


def build_cmd(args) -> list[str]:
    # seaborg is single-threaded and exposes no `Threads` option, so `Threads=1`
    # is set only on Stockfish (which would otherwise default to many threads);
    # `Hash` is understood by both. Limits are per-engine so the two sides can
    # run at independent node/time budgets.
    cmd = [
        args.fastchess,
        "-engine", "name=seaborg", f"cmd={args.seaborg}", "proto=uci",
        *args.seaborg_limit.split(),
        "-engine", "name=stockfish", f"cmd={args.stockfish}", "proto=uci",
        "option.Threads=1", *args.sf_limit.split(),
        "-each", f"restart={getattr(args, 'restart', 'on')}", f"option.Hash={args.hash}",
        "-rounds", str(args.games // 2), "-games", "2", "-repeat", "2",
        "-concurrency", str(args.concurrency),
        "-openings", f"file={args.openings}", "format=epd", "order=random",
        "-ratinginterval", "0",
    ]
    if args.pgnout:
        cmd += ["-pgnout", f"file={args.pgnout}"]
    return cmd


def _num(token: str) -> float:
    if "inf" in token:
        return float("-inf") if token.startswith("-") else float("inf")
    return float(token)


def parse_result(text: str):
    """Parse fastchess's end-of-match summary block."""
    elo = elo_err = points_pct = None
    wld = penta = None
    m = re.search(r"Elo:\s*(-?[\d.]+|-?inf)\s*\+/-\s*([\d.]+|nan)", text)
    if m:
        elo = _num(m.group(1))
        elo_err = float("nan") if m.group(2) == "nan" else float(m.group(2))
    m = re.search(r"Games:\s*(\d+),\s*Wins:\s*(\d+),\s*Losses:\s*(\d+),\s*Draws:\s*(\d+)", text)
    if m:
        wld = (int(m.group(2)), int(m.group(3)), int(m.group(4)))
    m = re.search(r"Points:\s*[\d.]+\s*\(([\d.]+)\s*%\)", text)
    if m:
        points_pct = float(m.group(1))
    m = re.search(r"Ptnml\(0-2\):\s*\[(\d+),\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\]", text)
    if m:
        penta = tuple(int(m.group(i)) for i in range(1, 6))
    return elo, elo_err, wld, points_pct, penta


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fastchess", default="fastchess")
    ap.add_argument("--seaborg", required=True)
    ap.add_argument("--stockfish", required=True)
    ap.add_argument("--seaborg-limit", required=True)
    ap.add_argument("--sf-limit", required=True)
    ap.add_argument("--games", type=int, default=400)
    ap.add_argument("--concurrency", type=int, default=8)
    ap.add_argument("--hash", type=int, default=64)
    ap.add_argument("--restart", default="on", choices=["on", "off"],
                    help="restart engines between games (off is faster for many short games)")
    ap.add_argument("--openings", required=True)
    ap.add_argument("--pgnout", default=None)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    cmd = build_cmd(args)
    print("running:", " ".join(cmd), flush=True)
    proc = subprocess.run(cmd, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    elo, err, wld, pct, penta = parse_result(out)
    print(out[-1200:])
    print("\n=== parsed ===")
    print(f"seaborg limit: {args.seaborg_limit}   sf limit: {args.sf_limit}")
    if wld:
        w, l, d = wld
        print(f"W-L-D (seaborg): {w}-{l}-{d}  ({w + l + d} games)")
    if pct is not None:
        print(f"seaborg score: {pct:.2f}%")
    if penta is not None:
        print(f"Ptnml(0-2): {penta}")
    if elo is not None:
        print(f"Elo (seaborg - stockfish): {elo:+.1f} +/- {err:.1f}")

    if args.out:
        import json
        with open(args.out, "w", encoding="utf-8") as fh:
            json.dump(
                {
                    "seaborg_limit": args.seaborg_limit,
                    "sf_limit": args.sf_limit,
                    "games": args.games,
                    "wld": wld,
                    "score_pct": pct,
                    "pentanomial": penta,
                    "elo": None if elo is None else (elo if elo not in (float("inf"), float("-inf")) else str(elo)),
                    "elo_err": None if err is None else (err if err == err else "nan"),
                },
                fh,
                indent=2,
            )


if __name__ == "__main__":
    main()
