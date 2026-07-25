#!/usr/bin/env python3
"""NPS and effective-branching-factor diagnostic (TASK-82, AC#1 and AC#2).

Drives two UCI engines over a fixed single-core position suite and reports, per
engine:

* raw speed: nodes per second (aggregate = total nodes / total search time);
* effective branching factor: EBF = nodes**(1/depth), reported per position and
  as a geometric mean, plus nodes-to-reach-a-fixed-depth;
* depth reached at a fixed movetime (this one figure folds NPS and EBF
  together: it is what actually determines search depth per move).

Single thread on both engines so the comparison isolates the engine, not the
scheduler. Run on an idle machine; NPS is sensitive to CPU contention.

Usage:
    python3 nps_ebf.py --seaborg PATH --stockfish PATH \
        --suite bench-positions.epd --depth 13 --movetime 2000 \
        --repeats 3 --hash 64 --out nps_ebf.json
"""

from __future__ import annotations

import argparse
import json
import math
import time

from uci import Engine, SearchResult, load_fens


def geomean(values: list[float]) -> float:
    values = [v for v in values if v > 0]
    if not values:
        return 0.0
    return math.exp(sum(math.log(v) for v in values) / len(values))


def ebf(nodes: int, depth: int) -> float:
    if nodes <= 0 or depth <= 0:
        return 0.0
    return nodes ** (1.0 / depth)


def best_of(runs: list[SearchResult]) -> SearchResult:
    """Fastest run wins: least wall time for the same fixed-depth node tree.

    Node counts at a fixed depth are deterministic for a given engine, so the
    only thing that varies run to run is wall time (hence NPS). Taking the
    fastest run discounts scheduler noise on a busy host.
    """
    return min(runs, key=lambda r: (r.time_ms if r.time_ms > 0 else 1 << 60))


def measure_depth(engine: Engine, suite, depth: int, repeats: int):
    rows = []
    for fen, label in suite:
        engine.new_game()
        runs = [engine.go_depth(fen, depth) for _ in range(repeats)]
        r = best_of(runs)
        rows.append(
            {
                "fen": fen,
                "label": label,
                "depth": r.depth,
                "nodes": r.nodes,
                "time_ms": r.time_ms,
                "nps": r.nps,
                "ebf": ebf(r.nodes, r.depth),
            }
        )
    return rows


def measure_movetime(engine: Engine, suite, movetime_ms: int, repeats: int):
    rows = []
    for fen, label in suite:
        engine.new_game()
        runs = [engine.go_movetime(fen, movetime_ms) for _ in range(repeats)]
        # Depth reached is the figure of merit here; take the best (deepest).
        r = max(runs, key=lambda x: x.depth)
        rows.append({"fen": fen, "label": label, "depth": r.depth, "nodes": r.nodes})
    return rows


def summarize(depth_rows, movetime_rows, depth: int, movetime_ms: int):
    total_nodes = sum(r["nodes"] for r in depth_rows)
    total_time = sum(r["time_ms"] for r in depth_rows)
    agg_nps = int(total_nodes / (total_time / 1000.0)) if total_time else 0
    return {
        "fixed_depth": depth,
        "aggregate_nps": agg_nps,
        "median_nps": int(sorted(r["nps"] for r in depth_rows)[len(depth_rows) // 2]),
        "geomean_ebf": geomean([r["ebf"] for r in depth_rows]),
        "mean_nodes_to_depth": int(total_nodes / len(depth_rows)),
        "movetime_ms": movetime_ms,
        "mean_depth_at_movetime": sum(r["depth"] for r in movetime_rows)
        / len(movetime_rows),
        "median_depth_at_movetime": sorted(r["depth"] for r in movetime_rows)[
            len(movetime_rows) // 2
        ],
    }


def run_engine(name, cmd, suite, depth, movetime_ms, repeats, hash_mb):
    engine = Engine([cmd], {"Threads": "1", "Hash": str(hash_mb)})
    try:
        depth_rows = measure_depth(engine, suite, depth, repeats)
        movetime_rows = measure_movetime(engine, suite, movetime_ms, repeats)
    finally:
        engine.quit()
    return {
        "name": name,
        "cmd": cmd,
        "summary": summarize(depth_rows, movetime_rows, depth, movetime_ms),
        "depth_rows": depth_rows,
        "movetime_rows": movetime_rows,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seaborg", required=True)
    ap.add_argument("--stockfish", required=True)
    ap.add_argument("--suite", required=True)
    ap.add_argument("--depth", type=int, default=13)
    ap.add_argument("--movetime", type=int, default=2000)
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--hash", type=int, default=64)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    suite = load_fens(args.suite)
    print(f"suite: {len(suite)} positions, fixed depth {args.depth}, "
          f"movetime {args.movetime} ms, best of {args.repeats}")

    results = []
    for name, cmd in (("seaborg", args.seaborg), ("stockfish", args.stockfish)):
        t0 = time.time()
        res = run_engine(name, cmd, suite, args.depth, args.movetime,
                         args.repeats, args.hash)
        s = res["summary"]
        print(f"\n=== {name} ===")
        print(f"  aggregate NPS       : {s['aggregate_nps']:,}")
        print(f"  median NPS          : {s['median_nps']:,}")
        print(f"  geomean EBF (d={args.depth}) : {s['geomean_ebf']:.3f}")
        print(f"  mean nodes to depth : {s['mean_nodes_to_depth']:,}")
        print(f"  median depth @ {args.movetime}ms : {s['median_depth_at_movetime']}")
        print(f"  (measured in {time.time() - t0:.1f}s)")
        results.append(res)

    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            json.dump({"depth": args.depth, "movetime_ms": args.movetime,
                       "results": results}, fh, indent=2)
        print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
