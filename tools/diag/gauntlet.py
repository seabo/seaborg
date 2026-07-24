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
    cmd = [
        args.fastchess,
        "-engine", "name=seaborg", f"cmd={args.seaborg}", "proto=uci",
        *args.seaborg_limit.split(),
        "-engine", "name=stockfish", f"cmd={args.stockfish}", "proto=uci",
        *args.sf_limit.split(),
        "-each", "restart=on", "option.Threads=1", f"option.Hash={args.hash}",
        "-rounds", str(args.games // 2), "-games", "2", "-repeat", "2",
        "-concurrency", str(args.concurrency),
        "-openings", f"file={args.openings}", "format=epd", "order=random",
        "-ratinginterval", "0",
    ]
    if args.pgnout:
        cmd += ["-pgnout", f"file={args.pgnout}"]
    return cmd


def parse_result(text: str):
    elo = elo_err = None
    m = re.search(r"Elo(?: difference)?:?\s*(-?\d+(?:\.\d+)?)\s*\+/-\s*(\d+(?:\.\d+)?)", text)
    if m:
        elo, elo_err = float(m.group(1)), float(m.group(2))
    wld = None
    m = re.search(r"Score of .*?:\s*(\d+)\s*-\s*(\d+)\s*-\s*(\d+)", text)
    if m:
        wld = tuple(int(m.group(i)) for i in (1, 2, 3))
    return elo, elo_err, wld


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
    ap.add_argument("--openings", required=True)
    ap.add_argument("--pgnout", default=None)
    args = ap.parse_args()

    cmd = build_cmd(args)
    print("running:", " ".join(cmd), flush=True)
    proc = subprocess.run(cmd, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    elo, err, wld = parse_result(out)
    print(out[-2000:])
    print("\n=== parsed ===")
    print(f"seaborg limit: {args.seaborg_limit}   sf limit: {args.sf_limit}")
    if wld:
        w, l, d = wld
        print(f"W-L-D (seaborg): {w}-{l}-{d}  ({w + l + d} games)")
    if elo is not None:
        print(f"Elo (seaborg - stockfish): {elo:+.1f} +/- {err:.1f}")


if __name__ == "__main__":
    main()
