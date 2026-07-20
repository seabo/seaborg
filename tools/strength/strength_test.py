#!/usr/bin/env python3
"""Reproducible FastChess SPRT strength-test orchestrator."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import select
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
# A time-based FastChess limit (equal wall-clock for both engines). Required by
# the authoritative mode; smoke mode may also use depth=/nodes= for speed.
TIME_LIMIT = re.compile(r"^(tc|st)=\S+$", re.I)
# Real game-ending failures. "makes an illegal move"/"loses on time" are the
# FastChess result phrasings; nonzero Timeouts/Crashed appear in its per-player
# summary. "Illegal PV move" is deliberately NOT matched: FastChess emits it as
# a harmless warning for a bad principal-variation line while the game finishes.
FAILURE_PATTERNS = re.compile(
    r"(makes an illegal move|loses on time|disconnects|connection stalls|"
    r"Timeouts:\s*[1-9]|Crashed:\s*[1-9])", re.I)


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
    p.add_argument("--runner", default="fastchess")
    p.add_argument("--openings", type=Path, default=root / "openings-v1.epd")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--mode", choices=("authoritative", "smoke"),
                   default="authoritative")
    p.add_argument("--elo0", type=float, default=-5.0)
    p.add_argument("--elo1", type=float, default=0.0)
    p.add_argument("--alpha", type=probability, default=0.05)
    p.add_argument("--beta", type=probability, default=0.05)
    p.add_argument("--max-games", type=positive, default=10000)
    p.add_argument("--limit", default="tc=10+0.1",
                   help="FastChess resource limit applied equally to both "
                        "engines, e.g. tc=10+0.1, st=0.5, depth=8, nodes=200000; "
                        "authoritative mode requires a time-based limit")
    p.add_argument("--concurrency", type=positive, default=1)
    p.add_argument("--threads", type=positive, default=1,
                   help="worker threads per engine, sent as the UCI Threads "
                        "option. seaborg currently runs a single worker and does "
                        "not advertise Threads, so it tolerates the option for "
                        "forward compatibility but does not parallelise its search "
                        "until Lazy SMP lands; keep this at 1 for seaborg")
    p.add_argument("--hash-mb", type=positive, default=64)
    p.add_argument("--engine-arg", action="append", default=[],
                   metavar="ARG",
                   help="command-line argument passed identically to both "
                        "engine executables; repeatable. Use =-prefixed form "
                        "for dash arguments, e.g. --engine-arg=-u for seaborg "
                        "UCI mode")
    p.add_argument("--engine-option", action="append", default=[],
                   metavar="NAME=VALUE")
    p.add_argument("--preflight-timeout", type=positive, default=10)
    p.add_argument("--match-timeout", type=positive, default=None,
                   help="optional wall-clock seconds after which a running match "
                        "is aborted and reported as INFRASTRUCTURE ERROR; guards "
                        "against a hung runner or engine")
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
    if args.mode == "authoritative" and not TIME_LIMIT.match(args.limit):
        raise InfrastructureError(
            "authoritative mode requires a time-based limit "
            f"(tc=... or st=...), got {args.limit!r}")
    if "=" not in args.limit or not args.limit.split("=", 1)[1]:
        raise InfrastructureError(f"invalid limit: {args.limit!r}")
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


def uci_preflight(engine: Path, engine_args: Sequence[str], timeout: int) -> dict[str, str]:
    """Drive a real UCI handshake, keeping stdin open until a move is returned.

    stdin stays open until ``bestmove`` arrives so engines that abort a search
    on EOF still produce a legal move, and the whole exchange is bounded by
    ``timeout``.
    """
    started = time.monotonic()
    try:
        proc = subprocess.Popen(
            [str(engine), *engine_args], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    except OSError as exc:
        raise InfrastructureError(f"UCI preflight failed for {engine}: {exc}") from exc
    collected: list[str] = []
    bestmove: str | None = None
    deadline = started + timeout
    with proc:  # closes the engine's pipes on exit
        try:
            assert proc.stdin and proc.stdout
            proc.stdin.write("uci\nisready\nucinewgame\nposition startpos\ngo depth 4\n")
            proc.stdin.flush()
            while bestmove is None:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise InfrastructureError(f"UCI preflight timed out for {engine}")
                if not select.select([proc.stdout], [], [], remaining)[0]:
                    continue
                line = proc.stdout.readline()
                if not line:
                    break
                collected.append(line)
                match = re.match(r"bestmove\s+(\S+)", line)
                if match:
                    bestmove = match.group(1)
            try:
                proc.stdin.write("quit\n")
                proc.stdin.flush()
                proc.wait(timeout=5)
            except (BrokenPipeError, OSError, subprocess.TimeoutExpired):
                pass
        finally:
            if proc.poll() is None:
                proc.kill()
    output = "".join(collected)
    if "uciok" not in output or "readyok" not in output:
        raise InfrastructureError(f"incomplete UCI handshake for {engine}")
    if not bestmove or not re.fullmatch(r"(?:[a-h][1-8]){2}[qrbn]?", bestmove):
        raise InfrastructureError(f"invalid or missing preflight bestmove for {engine}")
    return {"bestmove": bestmove, "duration_seconds": f"{time.monotonic()-started:.3f}"}


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
    # restart=on makes FastChess restart each engine process between games so no
    # stale in-process state (e.g. the transposition table) can leak across the
    # games of a pair or between pairs. This backs the report's
    # restart_between_games claim and the task's between-game isolation
    # requirement; FastChess defaults restart to off.
    each = ["proto=uci", "restart=on", args.limit,
            f"option.Hash={args.hash_mb}", f"option.Threads={args.threads}"]
    each.extend(f"option.{item}" for item in args.engine_option)
    engine_args = " ".join(args.engine_arg)
    engines: list[str] = []
    for name, path in (("candidate", args.candidate), ("baseline", args.baseline)):
        engines += ["-engine", f"name={name}", f"cmd={path}"]
        if engine_args:
            engines.append(f"args={engine_args}")
    return [args.runner, *engines, "-each", *each,
            # -rounds counts opening pairs; -games 2 -repeat 2 plays each opening
            # twice with colours reversed. Total games = rounds * 2 = max_games
            # (validated even), so cap accounting stays consistent.
            "-rounds", str(args.max_games // 2), "-games", "2", "-repeat", "2",
            "-openings", f"file={args.openings}", "format=epd", "order=sequential",
            "-concurrency", str(args.concurrency),
            "-sprt", f"elo0={args.elo0}", f"elo1={args.elo1}",
            f"alpha={args.alpha}", f"beta={args.beta}",
            "-pgnout", f"file={pgn}"]


def parse_result(output: str) -> Result:
    if FAILURE_PATTERNS.search(output):
        raise InfrastructureError(
            "runner reported a crash, forfeit, illegal move, or time loss")
    scores = re.findall(
        r"Games:\s*(\d+),\s*Wins:\s*(\d+),\s*Losses:\s*(\d+),\s*Draws:\s*(\d+)",
        output)
    states = re.findall(
        r"LLR:\s*([-+\d.eE]+)\s*\([^)]*\)\s*\(\s*([-+\d.eE]+),\s*([-+\d.eE]+)\s*\)",
        output)
    if not scores or not states:
        raise InfrastructureError("malformed runner output: missing Games or LLR line")
    games, wins, losses, draws = map(int, scores[-1])
    llr, lower, upper = map(float, states[-1])
    if not all(math.isfinite(v) for v in (llr, lower, upper)) or lower >= upper:
        raise InfrastructureError("malformed runner output: invalid SPRT bounds")
    if games != wins + losses + draws:
        raise InfrastructureError("malformed runner output: game totals disagree")
    elo = re.findall(r"\bElo:\s*([-+\d.]+)\s*\+/-\s*([-+\d.]+|nan)", output, re.I)
    ptnml = re.findall(
        r"Ptnml\(0-2\):\s*\[(\d+),\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\]", output)
    elo_value = float(elo[-1][0]) if elo else None
    elo_error = None
    if elo and elo[-1][1].lower() != "nan":
        elo_error = float(elo[-1][1])
    return Result(games, wins, draws, losses, llr, lower, upper, elo_value, elo_error,
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
        preflight = {
            "baseline": uci_preflight(args.baseline, args.engine_arg, args.preflight_timeout),
            "candidate": uci_preflight(args.candidate, args.engine_arg, args.preflight_timeout)}
        # Reserve an immutable artifact directory before the expensive match.
        args.output.mkdir(parents=True, exist_ok=False)
        output_created = True
        pgn = args.output / "games.pgn"
        command = build_command(args, pgn)
        time_based = bool(TIME_LIMIT.match(args.limit))
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
            "settings": {"resource_limit": args.limit,
                         "time_control": args.limit if time_based else None,
                         "concurrency": args.concurrency,
                         "threads_per_engine": args.threads, "hash_mb_per_engine": args.hash_mb,
                         "engine_args": args.engine_arg, "engine_options": args.engine_option,
                         "restart_between_games": True,
                         "paired_colour_reversal": True, "max_games": args.max_games},
            "sprt": {"elo0": args.elo0, "elo1": args.elo1,
                     "alpha": args.alpha, "beta": args.beta},
            "preflight": preflight, "command": shlex.join(command),
            "artifacts": {"runner_log": "runner.log", "games_pgn": "games.pgn"},
            "verdict": "INFRASTRUCTURE ERROR"}
        try:
            # Run inside the artifact dir so the runner's own config dump
            # (FastChess writes config.json to its cwd) is archived, not left
            # in the repository.
            proc = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT, check=False,
                                  cwd=str(args.output), timeout=args.match_timeout)
        except subprocess.TimeoutExpired as exc:
            # On timeout the partial output is returned undecoded even with
            # text=True, so it may be bytes.
            partial = exc.output or ""
            if isinstance(partial, bytes):
                partial = partial.decode(errors="replace")
            (args.output / "runner.log").write_text(partial)
            raise InfrastructureError(
                f"match exceeded --match-timeout of {args.match_timeout}s") from exc
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
