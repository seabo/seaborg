"""Mate-find-rate diagnostic: does the engine, at normal search settings, find
forced mates that provably exist?

Reads the rules-verified forced-mate suite produced by the `mate_suite_gen`
example (positions where the side to move has a game-theoretic forced mate,
proven by movegen independently of the search). It runs a given network at a
normal blitz per-move budget and, broken down by mate distance, reports how often
the engine:

  * *plays a mating move* — its chosen move keeps the forced mate (checked against
    the suite's proven winning moves, so this is rules-verified, not a matter of
    trusting the engine's own score); and
  * *reports the mate* — its final search score is a mate for the side to move.

A find rate that falls off as the mate gets deeper is direct evidence of search
over-pruning: the engine's nominal depth covers the mate, but pruning discards the
winning line. A high rate rules out a gross tactical hole.

The network under test is selected with the engine's `EvalFile` UCI option, so no
rebuild is needed to compare networks. Stdlib only, driving the engine through the
shared UCI harness.

Example:

    python3 mate_find_rate.py --seaborg target/release/seaborg \
        --suite ../../suites/mate_suite.json --network net.sbnn --movetime 200
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass

from uci import Engine


SCORE_RE = re.compile(r"\bscore\s+(cp|mate)\s+(-?\d+)")
BESTMOVE_RE = re.compile(r"^bestmove\s+(\S+)")


def parse_bestmove(lines: list[str]) -> str | None:
    """The move from the terminating ``bestmove`` line, or ``None`` if absent."""
    for line in lines:
        match = BESTMOVE_RE.match(line.strip())
        if match:
            move = match.group(1)
            return None if move in ("(none)", "0000") else move
    return None


def parse_final_score(lines: list[str]) -> tuple[str, int] | None:
    """The score of the last ``info`` line that carries one, as ``(kind, value)``
    with ``kind`` in ``{"cp", "mate"}``. ``None`` if no scored info line appears.

    The deepest completed iteration is the last one printed before ``bestmove``,
    so its score is the engine's final verdict on the position.
    """
    final: tuple[str, int] | None = None
    for line in lines:
        if not line.startswith("info"):
            continue
        match = SCORE_RE.search(line)
        if match:
            final = (match.group(1), int(match.group(2)))
    return final


def reports_mate(score: tuple[str, int] | None) -> bool:
    """Whether ``score`` is a mate in favour of the side to move."""
    return score is not None and score[0] == "mate" and score[1] > 0


@dataclass
class Outcome:
    """The measured result on one suite position."""

    mate_moves: int
    played_mating_move: bool
    reported_mate: bool


def evaluate(position: dict, bestmove: str | None, score: tuple[str, int] | None) -> Outcome:
    """Score one position: did the engine keep the mate, and did it see it?

    "Kept the mate" is decided against the suite's proven winning moves, not the
    engine's own score, so it is a rules-verified success criterion.
    """
    winning = set(position["winning_first_moves"])
    return Outcome(
        mate_moves=position["mate_moves"],
        played_mating_move=bestmove is not None and bestmove in winning,
        reported_mate=reports_mate(score),
    )


def aggregate(outcomes: list[Outcome]) -> dict:
    """Pool outcomes into per-distance and overall find rates."""
    by_distance: dict[int, dict] = {}
    for outcome in outcomes:
        bucket = by_distance.setdefault(
            outcome.mate_moves, {"total": 0, "played": 0, "reported": 0}
        )
        bucket["total"] += 1
        bucket["played"] += int(outcome.played_mating_move)
        bucket["reported"] += int(outcome.reported_mate)

    buckets = []
    total = played = reported = 0
    for moves in sorted(by_distance):
        b = by_distance[moves]
        total += b["total"]
        played += b["played"]
        reported += b["reported"]
        buckets.append(
            {
                "mate_moves": moves,
                "total": b["total"],
                "played_mating_move": b["played"],
                "reported_mate": b["reported"],
                "play_rate": b["played"] / b["total"] if b["total"] else 0.0,
                "report_rate": b["reported"] / b["total"] if b["total"] else 0.0,
            }
        )
    return {
        "by_distance": buckets,
        "overall": {
            "total": total,
            "played_mating_move": played,
            "reported_mate": reported,
            "play_rate": played / total if total else 0.0,
            "report_rate": reported / total if total else 0.0,
        },
    }


def format_summary(agg: dict, movetime_desc: str) -> str:
    """Human-readable table of the find rates."""
    lines = [f"mate-find rate @ {movetime_desc}"]
    lines.append(f"{'mate in':>8}  {'n':>4}  {'plays mate':>12}  {'reports mate':>14}")
    for row in agg["by_distance"]:
        lines.append(
            f"{row['mate_moves']:>8}  {row['total']:>4}  "
            f"{row['play_rate'] * 100:>10.1f}%  {row['report_rate'] * 100:>12.1f}%"
        )
    o = agg["overall"]
    lines.append(
        f"{'all':>8}  {o['total']:>4}  {o['play_rate'] * 100:>10.1f}%  "
        f"{o['report_rate'] * 100:>12.1f}%"
    )
    return "\n".join(lines)


def measure(engine: Engine, positions: list[dict], go: str) -> list[Outcome]:
    """Run every suite position under ``go`` and score the engine's answer."""
    outcomes = []
    for position in positions:
        engine.new_game()
        engine.set_position(position["fen"])
        lines = engine.command(go, "bestmove")
        bestmove = parse_bestmove(lines)
        score = parse_final_score(lines)
        outcomes.append(evaluate(position, bestmove, score))
    return outcomes


def build_go(args: argparse.Namespace) -> tuple[str, str]:
    """The UCI ``go`` command and a human description of the search limit.

    A per-move movetime is the default: it stands in for the time a real blitz
    game affords a single move, which is the setting whose over-pruning this
    diagnostic is meant to expose. Fixed depth or nodes are offered for
    controlled comparisons where wall-clock should not vary.
    """
    if args.depth:
        return f"go depth {args.depth}", f"depth {args.depth}"
    if args.nodes:
        return f"go nodes {args.nodes}", f"{args.nodes} nodes"
    return f"go movetime {args.movetime}", f"{args.movetime}ms/move"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--seaborg", required=True, help="path to the engine binary")
    ap.add_argument("--suite", required=True, help="mate suite JSON from mate_suite_gen")
    ap.add_argument(
        "--network",
        default=None,
        help="SBNN network file to test via EvalFile; 'none' forces the "
        "hand-crafted evaluation; omit to use the binary's embedded network",
    )
    ap.add_argument("--movetime", type=int, default=200, help="per-move milliseconds")
    ap.add_argument("--depth", type=int, default=0, help="fixed depth instead of movetime")
    ap.add_argument("--nodes", type=int, default=0, help="fixed nodes instead of movetime")
    ap.add_argument("--hash", type=int, default=64, help="transposition table size (MB)")
    ap.add_argument("--out", default=None, help="write full JSON results here")
    args = ap.parse_args()

    with open(args.suite, encoding="utf-8") as handle:
        suite = json.load(handle)
    positions = suite["positions"]
    if not positions:
        print(f"no positions in {args.suite}", file=sys.stderr)
        return 1

    options = {"Hash": str(args.hash), "Threads": "1"}
    if args.network is not None:
        options["EvalFile"] = args.network

    go, go_desc = build_go(args)
    engine = Engine([args.seaborg], options)
    try:
        outcomes = measure(engine, positions, go)
    finally:
        engine.quit()

    agg = aggregate(outcomes)
    print(format_summary(agg, go_desc))

    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            json.dump(
                {
                    "suite": args.suite,
                    "network": args.network,
                    "limit": go_desc,
                    "results": agg,
                },
                handle,
                indent=2,
            )
        print(f"\nwrote {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
