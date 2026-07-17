#!/usr/bin/env python3
"""Reproducible cutechess SPRT strength-test orchestrator."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence

PASS, FAIL, INCONCLUSIVE, INFRA_ERROR = range(4)
VERDICT_EXIT = {"PASS": PASS, "FAIL": FAIL, "INCONCLUSIVE": INCONCLUSIVE,
                "INFRASTRUCTURE ERROR": INFRA_ERROR}
SUITE_SHA256 = "eca44927b4cabdaa96cb9ab24a66c54e7c7444ac1c3e28d97b4436c110c4e275"
FAILURE_WORDS = re.compile(
    r"(disconnect|crash|illegal move|connection stalls|time forfeit|"
    r"lost on time|failed to start|doesn't respond|invalid result)", re.I)


class InfrastructureError(RuntimeError):
    pass


class InfrastructureArgumentParser(argparse.ArgumentParser):
    """Route command-line configuration errors through the infra verdict."""

    def error(self, message: str) -> None:
        raise InfrastructureError(f"invalid command line: {message}")


@dataclass
class Result:
    games: int
    wins: int
    draws: int
    losses: int
    llr: float
    lower_bound: float
    upper_bound: float
    elo: float | None = None
    elo_error: float | None = None
    pentanomial: list[int] | None = None
    # Reserved for AC #7 report enumeration. The orchestrator is fail-closed:
    # any crash or forfeit raises InfrastructureError before a Result exists, so
    # a completed Result always reports zero here. A future counting mode could
    # populate these; crashes/forfeits are otherwise recorded as the
    # INFRASTRUCTURE ERROR report's error message.
    forfeits: int = 0
    crashes: int = 0
    runner_finished: bool = False


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def positive(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return parsed


def probability(value: str) -> float:
    parsed = float(value)
    if not 0.0 < parsed < 1.0:
        raise argparse.ArgumentTypeError("must be between 0 and 1")
    return parsed


def parser() -> argparse.ArgumentParser:
    root = Path(__file__).resolve().parent
    p = InfrastructureArgumentParser(description=__doc__)
    p.add_argument("--baseline", required=True, type=Path)
    p.add_argument("--baseline-id", required=True,
                   help="immutable revision/build identity")
    p.add_argument("--candidate", required=True, type=Path)
    p.add_argument("--candidate-id", required=True,
                   help="immutable revision/build identity")
    p.add_argument("--build-settings", required=True,
                   help="exact optimized build command/flags/target")
    p.add_argument("--runner", default="cutechess-cli")
    p.add_argument("--openings", type=Path, default=root / "openings-v1.epd")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--mode", choices=("authoritative", "smoke"),
                   default="authoritative")
    p.add_argument("--elo0", type=float, default=-5.0)
    p.add_argument("--elo1", type=float, default=0.0)
    p.add_argument("--alpha", type=probability, default=0.05)
    p.add_argument("--beta", type=probability, default=0.05)
    p.add_argument("--max-games", type=positive, default=10000)
    p.add_argument("--time-control", default="10+0.1")
    p.add_argument("--concurrency", type=positive, default=1)
    p.add_argument("--threads", type=positive, default=1)
    p.add_argument("--hash-mb", type=positive, default=64)
    p.add_argument("--engine-option", action="append", default=[],
                   metavar="NAME=VALUE")
    p.add_argument("--preflight-timeout", type=positive, default=10)
    return p


def requested_output(argv: Sequence[str]) -> Path | None:
    """Recover --output for a report when full argument parsing fails."""
    for index, value in enumerate(argv):
        if value.startswith("--output="):
            candidate = value.partition("=")[2]
            return Path(candidate) if candidate else None
        if (value == "--output" and index + 1 < len(argv)
                and not argv[index + 1].startswith("-")):
            return Path(argv[index + 1])
    return None


def validate(args: argparse.Namespace) -> None:
    if args.elo0 >= args.elo1:
        raise InfrastructureError("elo0 must be less than elo1")
    if args.mode == "smoke" and args.max_games == 10000:
        args.max_games = 4
    if args.max_games % 2:
        raise InfrastructureError("max-games must be even for paired openings")
    if args.mode == "smoke" and args.max_games > 20:
        raise InfrastructureError("smoke mode is capped at 20 games")
    for item in args.engine_option:
        if "=" not in item or not item.split("=", 1)[0]:
            raise InfrastructureError(f"invalid engine option: {item!r}")
    for label in ("baseline", "candidate"):
        path = getattr(args, label).resolve()
        if not path.is_file() or not os.access(path, os.X_OK):
            raise InfrastructureError(f"{label} is not an executable file: {path}")
        setattr(args, label, path)
    args.openings = args.openings.resolve()
    if not args.openings.is_file():
        raise InfrastructureError(f"opening suite is missing: {args.openings}")
    actual = sha256(args.openings)
    if actual != SUITE_SHA256:
        raise InfrastructureError(
            f"opening suite checksum mismatch: expected {SUITE_SHA256}, got {actual}")
    args.output = args.output.resolve()
    if args.output.exists():
        raise InfrastructureError(f"output path already exists: {args.output}")


def uci_preflight(engine: Path, timeout: int) -> dict[str, str]:
    started = time.monotonic()
    try:
        proc = subprocess.run(
            [str(engine)], input="uci\nisready\nucinewgame\nposition startpos\ngo depth 1\nquit\n",
            text=True, capture_output=True, timeout=timeout, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise InfrastructureError(f"UCI preflight failed for {engine}: {exc}") from exc
    output = proc.stdout
    if proc.returncode != 0 or "uciok" not in output or "readyok" not in output:
        raise InfrastructureError(f"incomplete UCI handshake for {engine}")
    moves = re.findall(r"(?m)^bestmove\s+(\S+)", output)
    if not moves or not re.fullmatch(r"(?:[a-h][1-8]){2}[qrbn]?", moves[-1]):
        raise InfrastructureError(f"invalid or missing preflight bestmove for {engine}")
    return {"bestmove": moves[-1], "duration_seconds": f"{time.monotonic()-started:.3f}"}


def runner_version(runner: str) -> str:
    try:
        proc = subprocess.run([runner, "-version"], text=True, capture_output=True,
                              timeout=10, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise InfrastructureError(f"cannot execute runner {runner!r}: {exc}") from exc
    version = (proc.stdout + proc.stderr).strip()
    if proc.returncode or not version:
        raise InfrastructureError(f"cannot determine runner version for {runner!r}")
    return version


def build_command(args: argparse.Namespace, pgn: Path) -> list[str]:
    common = ["proto=uci", f"tc={args.time_control}", "restart=on",
              f"option.Hash={args.hash_mb}", f"option.Threads={args.threads}"]
    common.extend(f"option.{item}" for item in args.engine_option)
    return [args.runner,
            "-engine", "name=candidate", f"cmd={args.candidate}",
            "-engine", "name=baseline", f"cmd={args.baseline}",
            "-each", *common,
            # -rounds counts opening pairs; -games 2 -repeat 2 plays each
            # opening twice with colours reversed. Total games = rounds * 2 =
            # max_games (validated even), so cap accounting stays consistent.
            "-tournament", "round-robin", "-rounds", str(args.max_games // 2),
            "-games", "2", "-repeat", "2",
            "-openings", f"file={args.openings}", "format=epd",
            "order=sequential", "policy=round",
            "-concurrency", str(args.concurrency),
            "-sprt", f"elo0={args.elo0}", f"elo1={args.elo1}",
            f"alpha={args.alpha}", f"beta={args.beta}",
            "-ratinginterval", "1", "-outcomeinterval", "1",
            "-pgnout", str(pgn), "fi"]


def parse_result(output: str) -> Result:
    if FAILURE_WORDS.search(output):
        raise InfrastructureError("runner reported a crash, forfeit, or protocol failure")
    scores = re.findall(
        r"Score of candidate vs baseline:\s*(\d+)\s*-\s*(\d+)\s*-\s*(\d+)", output)
    states = re.findall(
        r"SPRT:\s*llr\s+([-+\d.eE]+).*?lbound\s+([-+\d.eE]+).*?ubound\s+([-+\d.eE]+)",
        output, re.I)
    if not scores or not states:
        raise InfrastructureError("malformed runner output: missing score or SPRT state")
    wins, losses, draws = map(int, scores[-1])
    llr, lower, upper = map(float, states[-1])
    if not all(math.isfinite(v) for v in (llr, lower, upper)) or lower >= upper:
        raise InfrastructureError("malformed runner output: invalid SPRT bounds")
    ratings = re.findall(r"candidate\s+([-+\d.]+)\s+([-+\d.]+)\s+\d+", output)
    ptnml = re.findall(r"Ptnml\(0-2\):\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)", output, re.I)
    return Result(wins + draws + losses, wins, draws, losses, llr, lower, upper,
                  *(map(float, ratings[-1]) if ratings else (None, None)),
                  list(map(int, ptnml[-1])) if ptnml else None,
                  runner_finished="Finished match" in output)


def verdict(result: Result, cap: int, authoritative: bool) -> str:
    if not result.runner_finished:
        raise InfrastructureError("runner output is incomplete")
    if result.games > cap:
        raise InfrastructureError("runner exceeded configured game cap")
    if result.llr >= result.upper_bound:
        return "PASS" if authoritative else "INCONCLUSIVE"
    if result.llr <= result.lower_bound:
        return "FAIL" if authoritative else "INCONCLUSIVE"
    return "INCONCLUSIVE"


def run(argv: Sequence[str] | None = None) -> int:
    raw_argv = list(sys.argv[1:] if argv is None else argv)
    report: dict = {"schema_version": 1, "verdict": "INFRASTRUCTURE ERROR"}
    output_created = False
    try:
        args = parser().parse_args(raw_argv)
        validate(args)
        version = runner_version(args.runner)
        preflight = {"baseline": uci_preflight(args.baseline, args.preflight_timeout),
                     "candidate": uci_preflight(args.candidate, args.preflight_timeout)}
        # Reserve an immutable artifact directory before the expensive match.
        args.output.mkdir(parents=True, exist_ok=False)
        output_created = True
        pgn = args.output / "games.pgn"
        command = build_command(args, pgn)
        report = {
            "schema_version": 1, "mode": args.mode,
            "authority": "AUTHORITATIVE" if args.mode == "authoritative" else "NON-AUTHORITATIVE",
            "baseline": {"path": str(args.baseline), "identity": args.baseline_id,
                         "sha256": sha256(args.baseline)},
            "candidate": {"path": str(args.candidate), "identity": args.candidate_id,
                          "sha256": sha256(args.candidate)},
            "build_settings": args.build_settings, "runner": args.runner,
            "runner_version": version,
            "openings": {"path": str(args.openings), "sha256": sha256(args.openings),
                         "identity": "seaborg-openings-v1"},
            "settings": {"time_control": args.time_control, "concurrency": args.concurrency,
                         "threads_per_engine": args.threads, "hash_mb_per_engine": args.hash_mb,
                         "engine_options": args.engine_option, "restart_between_games": True,
                         "paired_colour_reversal": True, "max_games": args.max_games},
            "sprt": {"elo0": args.elo0, "elo1": args.elo1,
                     "alpha": args.alpha, "beta": args.beta},
            "preflight": preflight, "command": shlex.join(command),
            "artifacts": {"runner_log": "runner.log", "games_pgn": "games.pgn"},
            "verdict": "INFRASTRUCTURE ERROR"}
        proc = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, check=False)
        raw = proc.stdout
        (args.output / "runner.log").write_text(raw)
        result = parse_result(raw)
        final = verdict(result, args.max_games, args.mode == "authoritative")
        if proc.returncode != 0:
            raise InfrastructureError(f"runner exited {proc.returncode}")
        report["sprt"].update({"llr": result.llr,
                               "lower_bound": result.lower_bound,
                               "upper_bound": result.upper_bound})
        report.update({"results": asdict(result), "runner_exit_code": proc.returncode,
                       "verdict": final})
    except (InfrastructureError, OSError, ValueError) as exc:
        report["error"] = str(exc)
        final = "INFRASTRUCTURE ERROR"
    output_arg = (getattr(args, "output", None) if "args" in locals()
                  else requested_output(raw_argv))
    if output_arg and (output_created or not output_arg.exists()):
        output = output_arg.resolve()
        output.mkdir(parents=True, exist_ok=True)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    label = report.get("authority", "UNKNOWN")
    print(f"{label} strength test verdict: {final}")
    sprt = report.get("sprt", {})
    results = report.get("results", {})
    if all(field in sprt for field in ("llr", "lower_bound", "upper_bound")) \
            and "games" in results:
        print(f"SPRT elo0={sprt['elo0']} elo1={sprt['elo1']} "
              f"alpha={sprt['alpha']} beta={sprt['beta']} LLR={sprt['llr']} "
              f"bounds=[{sprt['lower_bound']}, {sprt['upper_bound']}], "
              f"games={results['games']}")
    elif "error" in report:
        print(report["error"], file=sys.stderr)
    return VERDICT_EXIT[final]


if __name__ == "__main__":
    raise SystemExit(run())
