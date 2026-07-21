#!/usr/bin/env python3
"""Reinforcement-loop orchestration: generate, train, export, gate, promote.

One iteration of the loop turns the current best network into a candidate and
keeps the candidate only if it is stronger:

  1. generate self-play data with the current best network as the evaluator
     (generation 0 has no network and bootstraps from the hand-crafted
     evaluation);
  2. train a candidate network on that data and export it to the ``SBNN`` file
     the engine loads;
  3. gate the candidate against the current best with the repository
     strength-test SPRT harness, one ``seaborg`` binary playing both sides and
     told apart only by its ``EvalFile`` option;
  4. promote the candidate to current-best only if it passes, and record the
     decision and its attribution either way.

This module is the mechanism. It adds no numeric machinery of its own: the
datagen, trainer, exporter, and strength harness are the pieces it composes, and
it owns only the loop, the gate decision, and the bookkeeping. Running the
programme for real — many generations at a real node budget and authoritative
SPRT — is a separate exercise; here every external step is behind a small
[`Backend`] seam so the loop's own logic is exercised without hours of compute.

The self-play purity boundary (``docs/nnue-design-contract.md``) is preserved by
construction: the only evaluator a generation ever plays with is either the
engine's own hand-crafted evaluation (generation 0) or a network this loop itself
promoted from earlier self-play. Nothing external — no foreign engine, no game
database, no imported weights — enters the loop.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# The packed self-play format is an 8-byte stream header followed by fixed 32-byte
# records (engine/src/selfplay/format.rs). The sample count of a datagen file is
# therefore exact arithmetic on its size, with no dependence on parsing stdout.
SAMPLE_HEADER_SIZE = 8
SAMPLE_RECORD_SIZE = 32

# strength_test.py's exit statuses, which are its SPRT verdict. Only PASS gates a
# promotion; every other outcome, including an infrastructure error, leaves the
# current best in place (docs/strength-testing.md).
VERDICT_BY_EXIT = {
    0: "PASS",
    1: "FAIL",
    2: "INCONCLUSIVE",
    3: "INFRASTRUCTURE_ERROR",
}

# Files inside a run's state directory.
BEST_NETWORK = "best.sbnn"
BEST_MANIFEST = "best.json"
LEDGER = "ledger.jsonl"
NETWORKS_DIR = "networks"
ITERATIONS_DIR = "iterations"


class LoopError(RuntimeError):
    """A step failed in a way that stops the whole iteration.

    A gate that returns FAIL or INCONCLUSIVE is a normal outcome, not this: it
    decides the promotion. This is for a broken step — datagen, training, or
    export exiting non-zero, or a missing artifact — where continuing would
    record a meaningless result.
    """


def sha256(path: Path) -> str:
    """Content hash of a file, used as the stable identity of a network."""
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def network_id(path: Path, generation: int) -> str:
    """A human-readable, collision-resistant identity for a network file.

    Pairs the generation that produced it with a prefix of its content hash, so
    a ledger entry names both where a network came from and exactly which bytes
    it was.
    """
    return f"nnue:gen-{generation:03d}:sha256={sha256(path)[:16]}"


@dataclass
class GenerateResult:
    """The outcome of a datagen step: where the samples are and how many."""

    path: Path
    samples: int


@dataclass
class GateResult:
    """The outcome of the strength gate.

    ``verdict`` is the harness's SPRT decision; ``exit_code`` is its raw status
    (the source of the verdict). The measured strength delta — the point Elo and
    its ± error margin — is carried when the harness produced a report, and is
    absent when it could not (for example an infrastructure error before any
    games).
    """

    verdict: str
    exit_code: int
    output_dir: Path
    elo: Optional[float] = None
    elo_interval: Optional[float] = None
    games_played: Optional[int] = None

    @property
    def passed(self) -> bool:
        return self.verdict == "PASS"


@dataclass
class IterationResult:
    """Everything one iteration decided, plus the attribution record it wrote."""

    generation: int
    verdict: str
    promoted: bool
    candidate_network: Path
    best_network: Optional[Path]
    attribution: dict


@dataclass
class LoopConfig:
    """Where the pieces live and how each iteration is parameterized.

    The paths locate the engine binary, the Python interpreter that can import
    the trainer's dependencies (point ``python`` at the trainer venv for a real
    run), and the two repository tools this loop drives. The remaining fields are
    the per-iteration knobs the datagen, trainer, and gate steps consume.
    """

    state_dir: Path
    engine: Path
    trainer_dir: Path
    strength_script: Path
    python: str = sys.executable
    runner: Optional[str] = None

    # Datagen.
    games: int = 100
    nodes: int = 5_000
    datagen_extra: list = field(default_factory=list)

    # Training and export. ``generation`` is supplied automatically per iteration
    # so the trainer's lambda schedule advances across the loop; everything else
    # (epochs, hidden width, lambda ramp, learning rate) is passed through.
    train_extra: list = field(default_factory=list)
    export_extra: list = field(default_factory=list)

    # Strength gate.
    mode: str = "authoritative"
    limit: str = "tc=10+0.1"
    max_games: int = 10_000
    build_settings: str = "unspecified build"
    gate_extra: list = field(default_factory=list)


class Backend:
    """The external steps an iteration runs, behind a seam.

    Production wires these to real subprocesses ([`SubprocessBackend`]); tests
    substitute a fake so the loop's promotion, bootstrap, and bookkeeping logic
    is exercised without datagen, PyTorch, or FastChess.
    """

    def generate(
        self, out: Path, network: Optional[Path], nodes: int, games: int
    ) -> GenerateResult:
        raise NotImplementedError

    def train(self, data: Path, checkpoint: Path, generation: int) -> None:
        raise NotImplementedError

    def export(self, checkpoint: Path, network: Path) -> None:
        raise NotImplementedError

    def gate(
        self,
        baseline_network: Optional[Path],
        baseline_generation: Optional[int],
        candidate_network: Path,
        output_dir: Path,
        generation: int,
    ) -> GateResult:
        raise NotImplementedError


class SubprocessBackend(Backend):
    """Runs each step as a real subprocess, logging output under the iteration.

    Every command's combined output is teed to a log file beside its artifacts so
    a run leaves a complete trail. Datagen, training, and export must exit zero or
    the iteration stops; the gate is different — its non-zero exit is the SPRT
    verdict, not a failure — so its status is interpreted rather than enforced.
    """

    def __init__(self, config: LoopConfig):
        self.config = config

    def _run(self, command: list, log: Path, *, cwd: Optional[Path] = None) -> int:
        log.parent.mkdir(parents=True, exist_ok=True)
        with open(log, "wb") as handle:
            handle.write((" ".join(str(part) for part in command) + "\n\n").encode())
            handle.flush()
            completed = subprocess.run(
                [str(part) for part in command],
                cwd=str(cwd) if cwd else None,
                stdout=handle,
                stderr=subprocess.STDOUT,
                check=False,
            )
        return completed.returncode

    def generate(
        self, out: Path, network: Optional[Path], nodes: int, games: int
    ) -> GenerateResult:
        command = [
            self.config.engine,
            "datagen",
            "--games",
            games,
            "--nodes",
            nodes,
            "--out",
            out,
        ]
        if network is not None:
            command += ["--network", network]
        command += self.config.datagen_extra
        code = self._run(command, out.with_suffix(".datagen.log"))
        if code != 0:
            raise LoopError(f"datagen exited {code}; see {out.with_suffix('.datagen.log')}")
        if not out.is_file():
            raise LoopError(f"datagen produced no sample file at {out}")
        samples = (out.stat().st_size - SAMPLE_HEADER_SIZE) // SAMPLE_RECORD_SIZE
        return GenerateResult(path=out, samples=samples)

    def train(self, data: Path, checkpoint: Path, generation: int) -> None:
        command = [
            self.config.python,
            self.config.trainer_dir / "train.py",
            "--data",
            data,
            "--out",
            checkpoint,
            "--generation",
            generation,
        ] + self.config.train_extra
        # Run from the trainer directory so its sibling-module imports resolve.
        code = self._run(command, checkpoint.with_suffix(".train.log"), cwd=self.config.trainer_dir)
        if code != 0:
            raise LoopError(f"training exited {code}; see {checkpoint.with_suffix('.train.log')}")
        if not checkpoint.is_file():
            raise LoopError(f"training produced no checkpoint at {checkpoint}")

    def export(self, checkpoint: Path, network: Path) -> None:
        command = [
            self.config.python,
            self.config.trainer_dir / "export.py",
            "--checkpoint",
            checkpoint,
            "--out",
            network,
        ] + self.config.export_extra
        code = self._run(command, network.with_suffix(".export.log"), cwd=self.config.trainer_dir)
        if code != 0:
            raise LoopError(f"export exited {code}; see {network.with_suffix('.export.log')}")
        if not network.is_file():
            raise LoopError(f"export produced no network at {network}")

    def gate(
        self,
        baseline_network: Optional[Path],
        baseline_generation: Optional[int],
        candidate_network: Path,
        output_dir: Path,
        generation: int,
    ) -> GateResult:
        candidate_id = network_id(candidate_network, generation)
        # The baseline is the current best network — labelled with the generation
        # that actually produced it (``baseline_generation``), which is not in
        # general the previous one, since a rejected candidate promotes nothing.
        # A baseline of ``None`` is generation 0's hand-crafted bootstrap, expressed
        # by giving the baseline side no EvalFile option so it runs the default.
        baseline_id = (
            network_id(baseline_network, baseline_generation)
            if baseline_network is not None
            else "handcrafted"
        )
        command = [
            self.config.python,
            self.config.strength_script,
            "--baseline",
            self.config.engine,
            "--baseline-id",
            baseline_id,
            "--candidate",
            self.config.engine,
            "--candidate-id",
            candidate_id,
            "--build-settings",
            self.config.build_settings,
            "--output",
            output_dir,
            "--mode",
            self.config.mode,
            "--limit",
            self.config.limit,
            "--max-games",
            self.config.max_games,
            "--candidate-option",
            f"EvalFile={candidate_network.resolve()}",
        ]
        if baseline_network is not None:
            command += ["--baseline-option", f"EvalFile={baseline_network.resolve()}"]
        if self.config.runner is not None:
            command += ["--runner", self.config.runner]
        command += self.config.gate_extra

        # The harness reserves its own --output directory, so log beside it.
        code = self._run(command, output_dir.with_suffix(".gate.log"))
        verdict = VERDICT_BY_EXIT.get(code, "INFRASTRUCTURE_ERROR")
        return _gate_result_from_report(verdict, code, output_dir)


def _gate_result_from_report(verdict: str, code: int, output_dir: Path) -> GateResult:
    """Fold the harness's report.json, when present, into a [`GateResult`].

    A report is absent when the harness failed before writing one; the verdict
    still stands (from the exit code), only the measured delta is unavailable.
    """
    report_path = output_dir / "report.json"
    elo = interval = games = None
    if report_path.is_file():
        try:
            report = json.loads(report_path.read_text())
        except (OSError, json.JSONDecodeError):
            report = {}
        # The harness writes its parsed Result under "results" (strength_test.py:
        # report.update({"results": asdict(result), ...})); the ± Elo margin is
        # "elo_error", not an "elo_interval"/"elo_ci" pair. Reading the wrong keys
        # silently drops the measured delta the ledger exists to record.
        result = report.get("results", {}) if isinstance(report, dict) else {}
        elo = result.get("elo")
        interval = result.get("elo_error")
        games = result.get("games")
    return GateResult(
        verdict=verdict,
        exit_code=code,
        output_dir=output_dir,
        elo=elo,
        elo_interval=interval,
        games_played=games,
    )


class ReinforcementLoop:
    """Drives iterations against a [`Backend`] and owns the run's state directory.

    The state directory is the single record of a run: the current best network
    and its manifest, every generation's artifacts, and an append-only ledger of
    what each iteration decided and why. Nothing here writes into the repository;
    a run's outputs live entirely under ``state_dir``.
    """

    def __init__(self, config: LoopConfig, backend: Backend):
        self.config = config
        self.backend = backend

    def run(self, iterations: int) -> list:
        """Run ``iterations`` consecutive iterations, continuing an existing run.

        Generations are numbered from where the ledger left off, so a run can be
        resumed and its generation numbers stay monotonic and unique.
        """
        start = self._next_generation()
        results = []
        for offset in range(iterations):
            results.append(self.run_iteration(start + offset))
        return results

    def run_iteration(self, generation: int) -> IterationResult:
        state = self.config.state_dir
        iteration_dir = state / ITERATIONS_DIR / f"gen-{generation:03d}"
        iteration_dir.mkdir(parents=True, exist_ok=True)

        best = self._current_best()
        # The generation that actually produced ``best`` — read from its manifest,
        # not assumed to be ``generation - 1``. A generation whose candidate is
        # rejected promotes nothing, so after a rejection the current best is an
        # older network; labelling it with the previous iteration number would name
        # a generation that produced no network and give the same bytes a different
        # id each iteration. ``None`` at the bootstrap, before any promotion.
        baseline_generation = self._best_generation()

        samples = iteration_dir / "samples.bin"
        generated = self.backend.generate(
            out=samples,
            network=best,
            nodes=self.config.nodes,
            games=self.config.games,
        )

        checkpoint = iteration_dir / "candidate.pt"
        self.backend.train(generated.path, checkpoint, generation)

        candidate = iteration_dir / "candidate.sbnn"
        self.backend.export(checkpoint, candidate)

        gate = self.backend.gate(
            baseline_network=best,
            baseline_generation=baseline_generation,
            candidate_network=candidate,
            output_dir=iteration_dir / "strength",
            generation=generation,
        )

        promoted = gate.passed
        promoted_path = self._promote(generation, candidate) if promoted else None

        attribution = self._attribution(
            generation, best, baseline_generation, candidate, generated, gate, promoted
        )
        self._append_ledger(attribution)

        return IterationResult(
            generation=generation,
            verdict=gate.verdict,
            promoted=promoted,
            candidate_network=candidate,
            best_network=promoted_path if promoted else best,
            attribution=attribution,
        )

    def _current_best(self) -> Optional[Path]:
        """The promoted network to evaluate with, or ``None`` before any promotion.

        ``None`` is generation 0's hand-crafted bootstrap and is what preserves
        the purity boundary: an iteration only ever plays with a network this loop
        promoted earlier, never anything external.
        """
        best = self.config.state_dir / BEST_NETWORK
        return best if best.is_file() else None

    def _best_generation(self) -> Optional[int]:
        """The generation whose passing candidate became the current best.

        Read from ``best.json`` (written by [`_promote`]), this is the network's
        true origin — the identity ``network_id`` and the gate's ``--baseline-id``
        must name so the same bytes keep one stable id across iterations and agree
        with the manifest. ``None`` before any promotion, matching the hand-crafted
        bootstrap that has no producing generation.
        """
        manifest = self.config.state_dir / BEST_MANIFEST
        if not manifest.is_file():
            return None
        return json.loads(manifest.read_text())["generation"]

    def _next_generation(self) -> int:
        """One past the highest generation the ledger records, or 0 if empty."""
        ledger = self.config.state_dir / LEDGER
        if not ledger.is_file():
            return 0
        highest = -1
        for line in ledger.read_text().splitlines():
            line = line.strip()
            if not line:
                continue
            highest = max(highest, json.loads(line)["generation"])
        return highest + 1

    def _promote(self, generation: int, candidate: Path) -> Path:
        """Adopt a passing candidate as the new current best.

        The candidate is archived under ``networks/`` by generation and copied to
        the stable ``best.sbnn`` the next iteration reads, with a manifest naming
        which generation the best came from. Copies rather than a symlink so the
        pointer survives being moved or inspected on any platform.
        """
        state = self.config.state_dir
        archive = state / NETWORKS_DIR
        archive.mkdir(parents=True, exist_ok=True)
        archived = archive / f"gen-{generation:03d}.sbnn"
        shutil.copyfile(candidate, archived)

        best = state / BEST_NETWORK
        shutil.copyfile(candidate, best)
        (state / BEST_MANIFEST).write_text(
            json.dumps(
                {
                    "generation": generation,
                    "network_id": network_id(best, generation),
                    "sha256": sha256(best),
                },
                indent=2,
            )
            + "\n"
        )
        return best

    def _attribution(
        self,
        generation: int,
        best: Optional[Path],
        baseline_generation: Optional[int],
        candidate: Path,
        generated: GenerateResult,
        gate: GateResult,
        promoted: bool,
    ) -> dict:
        """Assemble the ledger record: data volume, node budget, ids, and delta.

        These are exactly what the strength harness and BENCHMARKS.md require to
        keep a strength change attributable — how much self-play data trained the
        candidate, at what node budget, which networks played, and the measured
        result of the gate.
        """
        return {
            "generation": generation,
            "timestamp": _timestamp(),
            "data": {
                "games": self.config.games,
                "samples": generated.samples,
                "node_budget": self.config.nodes,
            },
            "candidate": {
                "network_id": network_id(candidate, generation),
                "sha256": sha256(candidate),
                "path": str(candidate),
            },
            "baseline": {
                "network_id": network_id(best, baseline_generation)
                if best is not None
                else "handcrafted",
                "sha256": sha256(best) if best is not None else None,
                "bootstrap": best is None,
            },
            "gate": {
                "verdict": gate.verdict,
                "exit_code": gate.exit_code,
                "mode": self.config.mode,
                "limit": self.config.limit,
                "elo": gate.elo,
                "elo_interval": gate.elo_interval,
                "games_played": gate.games_played,
            },
            "promoted": promoted,
        }

    def _append_ledger(self, record: dict) -> None:
        ledger = self.config.state_dir / LEDGER
        ledger.parent.mkdir(parents=True, exist_ok=True)
        with open(ledger, "a") as handle:
            handle.write(json.dumps(record) + "\n")


def _timestamp() -> str:
    """UTC wall-clock, ISO-8601, for the ledger. Bookkeeping only."""
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def parser() -> argparse.ArgumentParser:
    root = Path(__file__).resolve().parents[2]
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--state", type=Path, required=True, help="run state directory")
    p.add_argument("--engine", type=Path, required=True, help="built seaborg release binary")
    p.add_argument("--iterations", type=int, default=1)
    p.add_argument("--python", default=sys.executable,
                   help="interpreter that can import the trainer deps (its venv for a real run)")
    p.add_argument("--trainer-dir", type=Path, default=root / "tools" / "trainer")
    p.add_argument("--strength-script", type=Path,
                   default=root / "tools" / "strength" / "strength_test.py")
    p.add_argument("--runner", default=None, help="FastChess binary if not on PATH")
    p.add_argument("--games", type=int, default=100)
    p.add_argument("--nodes", type=int, default=5_000)
    p.add_argument("--mode", choices=("authoritative", "smoke"), default="authoritative")
    p.add_argument("--limit", default="tc=10+0.1")
    p.add_argument("--max-games", type=int, default=10_000)
    p.add_argument("--build-settings", default="unspecified build")
    p.add_argument("--datagen-arg", action="append", default=[], metavar="ARG",
                   help="extra argument forwarded to seaborg datagen; repeatable")
    p.add_argument("--train-arg", action="append", default=[], metavar="ARG",
                   help="extra argument forwarded to train.py; repeatable")
    p.add_argument("--export-arg", action="append", default=[], metavar="ARG",
                   help="extra argument forwarded to export.py; repeatable")
    p.add_argument("--gate-arg", action="append", default=[], metavar="ARG",
                   help="extra argument forwarded to strength_test.py; repeatable")
    return p


def config_from_args(args: argparse.Namespace) -> LoopConfig:
    return LoopConfig(
        state_dir=args.state,
        engine=args.engine,
        trainer_dir=args.trainer_dir,
        strength_script=args.strength_script,
        python=args.python,
        runner=args.runner,
        games=args.games,
        nodes=args.nodes,
        datagen_extra=list(args.datagen_arg),
        train_extra=list(args.train_arg),
        export_extra=list(args.export_arg),
        mode=args.mode,
        limit=args.limit,
        max_games=args.max_games,
        build_settings=args.build_settings,
        gate_extra=list(args.gate_arg),
    )


def main(argv=None) -> int:
    args = parser().parse_args(argv)
    config = config_from_args(args)
    config.state_dir.mkdir(parents=True, exist_ok=True)
    loop = ReinforcementLoop(config, SubprocessBackend(config))
    try:
        results = loop.run(args.iterations)
    except LoopError as error:
        print(f"reinforcement loop stopped: {error}", file=sys.stderr)
        return 1

    for result in results:
        delta = result.attribution["gate"]["elo"]
        delta = f", Elo {delta:+.1f}" if isinstance(delta, (int, float)) else ""
        state = "promoted" if result.promoted else "kept best"
        print(f"gen {result.generation:03d}: {result.verdict} ({state}){delta}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())


# Re-exported for tests and callers that build records directly.
__all__ = [
    "Backend",
    "SubprocessBackend",
    "ReinforcementLoop",
    "LoopConfig",
    "LoopError",
    "GenerateResult",
    "GateResult",
    "IterationResult",
    "network_id",
    "sha256",
]
