"""Selectivity profile for Seaborg's own search.

Drives a `--features selstats` build of Seaborg over a fixed position suite and
collects the per-search `info string selstats {json}` line it emits. Aggregates
the counters into the selectivity signals that say where effective depth is
spent: first-move-cutoff rate (move ordering), LMR re-search rate and reduction
distribution (reduction calibration), PVS and aspiration re-search rates,
quiescence widening, and TT-move availability. Every figure comes from Seaborg's
own instrumentation; no other engine is consulted.

This measures ratios and node counts, which are independent of wall-clock speed,
so the instrumentation's own overhead does not bias them. Run it at a fixed
depth (identical trees per position) and at a fixed node budget (to read the
depth reached under a fixed effort). Depth-reached-at-fixed-time is better read
from an uninstrumented release, since instrumentation perturbs the clock.

Stdlib only, so it runs under the bare Python on any measurement host.

Example:

    SB=/path/to/seaborg-selstats           # cargo build --release --features selstats
    python3 selectivity_profile.py --seaborg "$SB" \
        --suite bench-positions.epd --depth 14 --nodes 2000000 \
        --hash 64 --out selectivity_profile.json
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field

from uci import Engine, load_fens


SELSTATS_TAG = "info string selstats "


def run_one(engine: Engine, fen: str, go: str) -> dict:
    """Run a single search and return the selectivity JSON it emitted.

    Sends the position then the `go` command, reads to the terminating
    `bestmove`, and decodes the last `selstats` line. Raises if the build emitted
    none — the usual cause is a binary compiled without the `selstats` feature.
    """
    engine.set_position(fen)
    lines = engine.command(go, "bestmove")
    payload = None
    for line in lines:
        idx = line.find(SELSTATS_TAG)
        if idx != -1:
            payload = line[idx + len(SELSTATS_TAG) :].strip()
    if payload is None:
        raise RuntimeError(
            "no `selstats` line emitted; build Seaborg with --features selstats"
        )
    return json.loads(payload)


@dataclass
class Pooled:
    """Running sums pooled across the suite, from which rates are derived.

    Pooling numerator and denominator across positions (rather than averaging
    per-position rates) weights each rate by the events that produced it, so a
    position that searched more nodes contributes proportionally more — the
    correct weighting for a "typical node" view of the search.
    """

    positions: int = 0
    nodes: int = 0
    qnodes: int = 0
    all_nodes: int = 0
    depth_sum: int = 0
    ebf_sum: float = 0.0
    hash_probes: int = 0
    hash_hits: int = 0
    hash_collisions: int = 0
    tt_cutoffs: int = 0
    tt_move_avail_weighted: float = 0.0
    nodes_pv: int = 0
    nodes_nonpv: int = 0
    fh_total: int = 0
    fh_first: int = 0
    fh_idx: list = field(default_factory=lambda: [0] * 8)
    pv_fail_high: int = 0
    nonpv_fail_high: int = 0
    pv_exact: int = 0
    pv_fail_low: int = 0
    nonpv_fail_low: int = 0
    ord_pv_tt: int = 0
    ord_pv_nott: int = 0
    ord_nonpv_tt: int = 0
    ord_nonpv_nott: int = 0
    fh_tt: int = 0
    fh_tt_first: int = 0
    fh_nott: int = 0
    fh_nott_first: int = 0
    lmr_applied: int = 0
    lmr_research: int = 0
    lmr_red_sum: int = 0
    lmr_red_hist: list = field(default_factory=lambda: [0] * 6)
    pv_scout: int = 0
    pv_scout_research: int = 0
    asp_windows: int = 0
    asp_fail_low: int = 0
    asp_fail_high: int = 0
    q_incheck: int = 0

    def add(self, s: dict) -> None:
        self.positions += 1
        self.nodes += s["nodes"]
        self.qnodes += s["qnodes"]
        self.all_nodes += s["all_nodes"]
        self.depth_sum += s["depth"]
        self.ebf_sum += s["ebf"]
        self.hash_probes += s["hash_probes"]
        self.hash_hits += s["hash_hits"]
        self.hash_collisions += s["hash_collisions"]
        self.tt_cutoffs += s["tt_cutoffs"]
        self.tt_move_avail_weighted += s["tt_move_avail"] * s["nodes"]
        self.nodes_pv += s["nodes_pv"]
        self.nodes_nonpv += s["nodes_nonpv"]
        self.fh_total += s["fh_total"]
        self.fh_first += s["fh_first"]
        for i, v in enumerate(s["fh_idx"]):
            self.fh_idx[i] += v
        self.pv_fail_high += s["pv_fail_high"]
        self.nonpv_fail_high += s["nonpv_fail_high"]
        self.pv_exact += s["pv_exact"]
        self.pv_fail_low += s["pv_fail_low"]
        self.nonpv_fail_low += s["nonpv_fail_low"]
        self.ord_pv_tt += s["ord_pv_tt"]
        self.ord_pv_nott += s["ord_pv_nott"]
        self.ord_nonpv_tt += s["ord_nonpv_tt"]
        self.ord_nonpv_nott += s["ord_nonpv_nott"]
        self.fh_tt += s["fh_tt"]
        self.fh_tt_first += s["fh_tt_first"]
        self.fh_nott += s["fh_nott"]
        self.fh_nott_first += s["fh_nott_first"]
        self.lmr_applied += s["lmr_applied"]
        self.lmr_research += s["lmr_research"]
        self.lmr_red_sum += s["lmr_red_sum"]
        for i, v in enumerate(s["lmr_red_hist"]):
            self.lmr_red_hist[i] += v
        self.pv_scout += s["pv_scout"]
        self.pv_scout_research += s["pv_scout_research"]
        self.asp_windows += s["asp_windows"]
        self.asp_fail_low += s["asp_fail_low"]
        self.asp_fail_high += s["asp_fail_high"]
        self.q_incheck += s["q_incheck"]


def _rate(num: int, den: int) -> float:
    return num / den if den else 0.0


def derive(p: Pooled) -> dict:
    """Turn pooled sums into the named selectivity rates."""
    pv_loops = p.pv_fail_high + p.pv_exact + p.pv_fail_low
    nonpv_loops = p.nonpv_fail_high + p.nonpv_fail_low
    ord_pv = p.ord_pv_tt + p.ord_pv_nott
    ord_nonpv = p.ord_nonpv_tt + p.ord_nonpv_nott
    return {
        "positions": p.positions,
        "mean_depth": _rate(p.depth_sum, p.positions),
        "mean_ebf": _rate(p.ebf_sum, p.positions),
        "nodes_total": p.nodes,
        "qnodes_total": p.qnodes,
        "all_nodes_total": p.all_nodes,
        "q_fraction": _rate(p.qnodes, p.all_nodes),
        "q_incheck_fraction": _rate(p.q_incheck, p.qnodes),
        "hash_hit_rate": _rate(p.hash_hits, p.hash_probes),
        "tt_move_avail": _rate(p.tt_move_avail_weighted, p.nodes),
        "tt_cutoff_rate_nonpv": _rate(p.tt_cutoffs, p.nodes_nonpv),
        "pv_node_fraction": _rate(p.nodes_pv, p.nodes),
        "first_move_cutoff_rate": _rate(p.fh_first, p.fh_total),
        "cutoff_index_dist": [_rate(v, p.fh_total) for v in p.fh_idx],
        "tt_move_avail_pv": _rate(p.ord_pv_tt, ord_pv),
        "tt_move_avail_nonpv": _rate(p.ord_nonpv_tt, ord_nonpv),
        "first_move_cutoff_rate_tt": _rate(p.fh_tt_first, p.fh_tt),
        "first_move_cutoff_rate_nott": _rate(p.fh_nott_first, p.fh_nott),
        "cutoff_share_nott": _rate(p.fh_nott, p.fh_total),
        "pv_fail_high_rate": _rate(p.pv_fail_high, pv_loops),
        "pv_exact_rate": _rate(p.pv_exact, pv_loops),
        "pv_fail_low_rate": _rate(p.pv_fail_low, pv_loops),
        "nonpv_fail_high_rate": _rate(p.nonpv_fail_high, nonpv_loops),
        "nonpv_fail_low_rate": _rate(p.nonpv_fail_low, nonpv_loops),
        "lmr_applied": p.lmr_applied,
        "lmr_research_rate": _rate(p.lmr_research, p.lmr_applied),
        "lmr_mean_reduction": _rate(p.lmr_red_sum, p.lmr_applied),
        "lmr_reduction_dist": [_rate(v, p.lmr_applied) for v in p.lmr_red_hist],
        "pvs_research_rate": _rate(p.pv_scout_research, p.pv_scout),
        "aspiration_research_rate": _rate(
            p.asp_fail_low + p.asp_fail_high, p.asp_windows
        ),
    }


def profile(engine: Engine, fens: list, go: str) -> dict:
    """Run every position under `go` and return per-position rows plus aggregate."""
    pooled = Pooled()
    rows = []
    for fen, label in fens:
        engine.new_game()
        stats = run_one(engine, fen, go)
        pooled.add(stats)
        rows.append({"fen": fen, "label": label, "selstats": stats})
    return {"go": go, "aggregate": derive(pooled), "positions": rows}


def format_summary(mode: str, agg: dict) -> str:
    """Human-readable one-block summary of an aggregate, for the console."""
    lines = [f"== {mode} ({agg['positions']} positions) =="]
    lines.append(f"mean depth reached      {agg['mean_depth']:.2f}")
    lines.append(f"mean EBF                {agg['mean_ebf']:.3f}")
    lines.append(f"quiescence fraction     {agg['q_fraction'] * 100:.1f}%")
    lines.append(f"  of which in check     {agg['q_incheck_fraction'] * 100:.1f}%")
    lines.append(f"first-move-cutoff rate  {agg['first_move_cutoff_rate'] * 100:.1f}%")
    dist = " ".join(f"{v * 100:4.1f}" for v in agg["cutoff_index_dist"])
    lines.append(f"  cutoff move-idx %     [{dist}] (1,2,3,4,5,6,7,8+)")
    lines.append(f"non-PV node fraction    {(1 - agg['pv_node_fraction']) * 100:.2f}%")
    lines.append(f"TT-move availability    {agg['tt_move_avail'] * 100:.1f}%")
    lines.append(
        f"  PV / non-PV nodes     {agg['tt_move_avail_pv'] * 100:.1f}% / "
        f"{agg['tt_move_avail_nonpv'] * 100:.1f}%"
    )
    lines.append(
        f"first-move-cutoff by TT  hit {agg['first_move_cutoff_rate_tt'] * 100:.1f}% vs "
        f"miss {agg['first_move_cutoff_rate_nott'] * 100:.1f}% "
        f"(misses are {agg['cutoff_share_nott'] * 100:.0f}% of cutoffs)"
    )
    lines.append(f"hash hit rate           {agg['hash_hit_rate'] * 100:.1f}%")
    lines.append(f"non-PV TT-cutoff rate   {agg['tt_cutoff_rate_nonpv'] * 100:.1f}%")
    lines.append(
        f"LMR applied {agg['lmr_applied']}, re-search rate "
        f"{agg['lmr_research_rate'] * 100:.2f}%, mean reduction "
        f"{agg['lmr_mean_reduction']:.2f} ply"
    )
    rdist = " ".join(f"{v * 100:4.1f}" for v in agg["lmr_reduction_dist"])
    lines.append(f"  reduction-ply %       [{rdist}] (1,2,3,4,5,6+)")
    lines.append(f"PVS re-search rate      {agg['pvs_research_rate'] * 100:.2f}%")
    lines.append(
        f"PV exact/fail-low/high  {agg['pv_exact_rate'] * 100:.1f}% / "
        f"{agg['pv_fail_low_rate'] * 100:.1f}% / {agg['pv_fail_high_rate'] * 100:.1f}%"
    )
    lines.append(
        f"non-PV fail-high/low    {agg['nonpv_fail_high_rate'] * 100:.1f}% / "
        f"{agg['nonpv_fail_low_rate'] * 100:.1f}%"
    )
    lines.append(
        f"aspiration re-search    {agg['aspiration_research_rate'] * 100:.1f}% per window"
    )
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--seaborg", required=True, help="path to a --features selstats build")
    ap.add_argument("--suite", default="bench-positions.epd")
    ap.add_argument("--depth", type=int, default=14, help="fixed-depth run; 0 to skip")
    ap.add_argument("--nodes", type=int, default=2_000_000, help="fixed-nodes run; 0 to skip")
    ap.add_argument("--hash", type=int, default=64, help="transposition table size (MB)")
    ap.add_argument("--out", default=None, help="write full JSON here")
    args = ap.parse_args()

    fens = load_fens(args.suite)
    if not fens:
        print(f"no positions in {args.suite}", file=sys.stderr)
        return 1

    engine = Engine([args.seaborg], {"Hash": str(args.hash)})
    result = {"suite": args.suite, "hash_mb": args.hash, "runs": {}}
    try:
        if args.depth:
            result["runs"]["fixed_depth"] = profile(
                engine, fens, f"go depth {args.depth}"
            )
            print(format_summary(f"fixed depth {args.depth}", result["runs"]["fixed_depth"]["aggregate"]))
            print()
        if args.nodes:
            result["runs"]["fixed_nodes"] = profile(
                engine, fens, f"go nodes {args.nodes}"
            )
            print(format_summary(f"fixed nodes {args.nodes}", result["runs"]["fixed_nodes"]["aggregate"]))
    finally:
        engine.quit()

    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            json.dump(result, handle, indent=2)
        print(f"\nwrote {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
