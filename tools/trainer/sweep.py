"""The NNUE architecture-sweep screen (docs/nnue-architecture-sweep.md).

Choosing the next network is a funnel: a cheap screen ranks many candidate
*shapes* of the network on two axes — post-quantization validation loss (eval
quality) and realized single-thread NPS (eval cost) — and discards every shape
another shape beats on *both*; only the survivors on the loss/NPS Pareto frontier
earn the expensive fixed-time-control SPRT game matches that actually pick the
winner. This module is that screen and the finalist hand-off, not the game
matches: it enumerates the candidates, records each one's (loss, NPS) with
attribution, computes the frontier, selects finalists spanning the trade, and
emits the exact ``strength_test.py`` commands to run them against the current
default. Running the multi-day training and the thousands of SPRT games is the
downstream campaign; this driver stops at the commands.

The one rule the screen depends on is *fixed-everything-but-architecture*: loss
function, ``lambda``, epochs/lr/batch/optimizer/seed, corpus, and the leak-free
by-shard validation split are constants of the whole sweep (:class:`TrainingConfig`),
and only the architecture varies (:class:`Architecture`). A knob that drifted
between candidates would confound topology with training and silently invalidate
the comparison. Candidates are enumerated one factor at a time from a fixed
baseline so each point isolates a single architectural change.

The heavy operations — training-and-exporting a candidate and measuring its NPS —
are injected into :func:`run_sweep` as callables, so the enumeration, domination,
and finalist logic is pure and unit-tested without a GPU or the engine. The CLI
wires the real implementations: ``train.py`` + ``export.py`` for the network, and
a single-thread UCI bench over a fixed suite for the NPS.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
from dataclasses import asdict, dataclass, field, replace
from pathlib import Path
from typing import Callable, Optional

# The SBNN header stores an 8-byte little-endian FNV-1a hash of the parameter blob
# at this offset (engine/src/nnue/format.rs; tools/trainer/export.py `_OFF_PARAM_HASH`).
# Read locally so a candidate's network is attributable without importing the
# exporter (and its torch dependency) into the pure sweep logic.
_SBNN_PARAM_HASH_OFFSET = 32
_SBNN_PARAM_HASH_BYTES = 8

# The two activations with integer inference in both the exporter and the engine.
_ACTIVATIONS = ("crelu", "screlu")
# H and every hidden stack dim must be positive multiples of 16 so one file loads
# unchanged into the scalar and the 16-lane SIMD inference paths.
_WIDTH_MULTIPLE = 16


class SweepError(ValueError):
    """A candidate architecture or a measured result was malformed."""


# --------------------------------------------------------------------------- #
# Candidate architectures
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class Architecture:
    """One candidate network shape — the only thing that varies across the sweep.

    ``output_stack`` is the tuple of hidden output-stack dims (``None`` is the
    version-1 single linear output); ``num_buckets`` the piece-count-selected stack
    count (only meaningful with a stack); ``output_stack_scales`` the per-layer int8
    weight scales ``QB_k``, one per stack layer including the final ``-> 1`` (``None``
    defaults every layer to the sweep's ``qb``). The fields mirror ``train.py``'s
    architecture flags exactly, so :meth:`train_flags` maps a candidate to a training
    invocation without re-deriving anything."""

    hidden: int = 256
    activation: str = "crelu"
    num_buckets: int = 1
    output_stack: Optional[tuple[int, ...]] = None
    output_stack_scales: Optional[tuple[int, ...]] = None

    @property
    def is_bucketed(self) -> bool:
        return self.output_stack is not None

    def validate(self, qb: int) -> None:
        """Reject a shape the trainer or the file format would refuse, so an invalid
        candidate is caught at enumeration rather than after a training run. Mirrors
        the loader's rules in ``model.NnueConfig.validate`` for the architecture
        fields; ``qb`` is the sweep's output scale, which the final stack scale must
        match."""
        if self.hidden <= 0 or self.hidden % _WIDTH_MULTIPLE != 0:
            raise SweepError(f"hidden width {self.hidden} must be a positive multiple of 16")
        if self.activation not in _ACTIVATIONS:
            raise SweepError(f"unknown activation {self.activation!r}")
        if self.is_bucketed:
            if not 1 <= self.num_buckets <= 32:
                raise SweepError(f"num_buckets {self.num_buckets} must be in 1..=32")
            for d in self.output_stack:
                if d <= 0 or d % _WIDTH_MULTIPLE != 0:
                    raise SweepError(f"output-stack dim {d} must be a positive multiple of 16")
            if self.output_stack_scales is not None:
                layers = len(self.output_stack) + 1
                if len(self.output_stack_scales) != layers:
                    raise SweepError("output_stack_scales must have one entry per stack layer")
                if any(s <= 0 for s in self.output_stack_scales):
                    raise SweepError("every output-stack scale must be positive")
                if self.output_stack_scales[-1] != qb:
                    raise SweepError(f"final output-stack scale must equal qb {qb}")
        elif self.num_buckets != 1 or self.output_stack_scales is not None:
            raise SweepError("buckets and stack scales apply only to a bucketed network")

    def key(self) -> tuple:
        """A canonical, hashable identity used to deduplicate candidates that
        different factors produce the same shape for (e.g. the baseline width)."""
        return (self.hidden, self.activation, self.num_buckets, self.output_stack,
                self.output_stack_scales)

    def slug(self) -> str:
        """A filesystem- and id-safe description of the shape, unique per distinct
        architecture, used for artifact names and SPRT identities."""
        parts = [f"h{self.hidden}", self.activation]
        if self.is_bucketed:
            parts.append("stack" + "-".join(str(d) for d in self.output_stack))
            parts.append(f"b{self.num_buckets}")
            if self.output_stack_scales is not None:
                parts.append("qb" + "-".join(str(s) for s in self.output_stack_scales))
        else:
            parts.append("v1")
        return "_".join(parts)

    def train_flags(self) -> list[str]:
        """The ``train.py`` command-line flags that select this architecture."""
        flags = ["--hidden", str(self.hidden), "--activation", self.activation]
        if self.is_bucketed:
            flags += ["--num-buckets", str(self.num_buckets)]
            flags += ["--output-stack", ",".join(str(d) for d in self.output_stack)]
            if self.output_stack_scales is not None:
                flags += ["--output-stack-scales",
                          ",".join(str(s) for s in self.output_stack_scales)]
        return flags


@dataclass(frozen=True)
class Candidate:
    """One enumerated candidate: its architecture and the single factor it varies
    from the baseline, so the report reads as a one-factor-at-a-time sweep."""

    factor: str
    arch: Architecture

    @property
    def name(self) -> str:
        return f"{self.factor}__{self.arch.slug()}"


@dataclass(frozen=True)
class SweepGrid:
    """The candidate grid: a fixed baseline plus, for each architectural factor, the
    values that factor takes while everything else stays at its reference. The
    bucket, stack-depth, and tail-quantization factors need an output stack, so they
    branch from a version-2 reference (the baseline width and activation under
    ``reference_stack`` / ``reference_buckets``) rather than the version-1 baseline.

    ``tail_scale_factors`` are multipliers on ``qb`` applied to the non-final stack
    layers (the dense tail), holding the final layer at ``qb``; they screen how
    coarsely the tail can be quantized before quality suffers. ``qb`` is the fixed
    output scale of the whole sweep."""

    baseline: Architecture = Architecture(hidden=256, activation="crelu")
    widths: tuple[int, ...] = (128, 256, 384, 512)
    activations: tuple[str, ...] = _ACTIVATIONS
    reference_stack: tuple[int, ...] = (16, 32)
    reference_buckets: int = 8
    bucket_counts: tuple[int, ...] = (1, 4, 8, 16)
    stack_depths: tuple[tuple[int, ...], ...] = ((16,), (16, 32), (16, 32, 32))
    tail_scale_factors: tuple[float, ...] = (0.5, 2.0)
    qb: int = 64


def _tail_scales(stack: tuple[int, ...], factor: float, qb: int) -> tuple[int, ...]:
    """Per-layer int8 scales for ``stack`` with the non-final (dense-tail) layers
    scaled by ``factor`` and the final ``-> 1`` layer pinned at ``qb``."""
    non_final = tuple(max(1, round(qb * factor)) for _ in stack)
    return non_final + (qb,)


def enumerate_candidates(grid: SweepGrid = SweepGrid()) -> list[Candidate]:
    """The one-factor-at-a-time candidate list. Every candidate differs from the
    baseline (or the version-2 reference, for the stack-only factors) in exactly one
    architectural dimension; a shape two factors both reach is emitted once, under
    the first factor that produced it (so the baseline is never duplicated)."""
    seen: set[tuple] = set()
    out: list[Candidate] = []

    def add(factor: str, arch: Architecture) -> None:
        arch.validate(grid.qb)
        if arch.key() in seen:
            return
        seen.add(arch.key())
        out.append(Candidate(factor=factor, arch=arch))

    add("baseline", grid.baseline)
    for h in grid.widths:
        add("width", replace(grid.baseline, hidden=h))
    for activation in grid.activations:
        add("activation", replace(grid.baseline, activation=activation))

    reference = replace(
        grid.baseline, output_stack=grid.reference_stack, num_buckets=grid.reference_buckets
    )
    for buckets in grid.bucket_counts:
        add("buckets", replace(reference, num_buckets=buckets))
    for depth in grid.stack_depths:
        add("stack-depth", replace(reference, output_stack=depth))
    for factor in grid.tail_scale_factors:
        scales = _tail_scales(grid.reference_stack, factor, grid.qb)
        add("tail-quant", replace(reference, output_stack_scales=scales))

    return out


# --------------------------------------------------------------------------- #
# The screen: (loss, NPS) points and the Pareto frontier
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class Screen:
    """A candidate's screened point: the two axes plus the attribution that makes
    the measurement reproducible — the exported network's parameter hash (which
    discriminates two same-width nets) and the engine binary's commit."""

    candidate: Candidate
    val_loss: float
    nps: float
    param_hash: str
    net_path: str
    binary_commit: Optional[str] = None

    @property
    def name(self) -> str:
        return self.candidate.name


def dominates(a: Screen, b: Screen) -> bool:
    """``a`` dominates ``b`` when it is no worse on either axis — lower-or-equal loss
    and higher-or-equal NPS — and strictly better on at least one. A dominated
    candidate cannot win the objective (some other net is better on both screen
    axes), so it is discarded without a game match. Two points with identical (loss,
    NPS) do not dominate each other, so neither is dropped for the other's sake."""
    return (a.val_loss <= b.val_loss and a.nps >= b.nps
            and (a.val_loss < b.val_loss or a.nps > b.nps))


def pareto_frontier(screens: list[Screen]) -> list[Screen]:
    """The non-dominated screened points, in the input order. Every dominated
    candidate is dropped; ties (equal loss and NPS) all survive."""
    return [s for s in screens if not any(dominates(o, s) for o in screens if o is not s)]


def select_finalists(frontier: list[Screen], count: int = 3) -> list[Screen]:
    """A handful of frontier points spanning the loss/NPS trade — a fast-but-looser
    net, a slow-but-sharper net, and evenly spaced middles — since the frontier's
    extremes bracket the trade and a middle point tests the knee. Points are ordered
    by NPS (ties broken toward lower loss) and sampled at even positions including
    both ends; exact-duplicate points collapse to one. Returns every frontier point
    when the frontier is no larger than ``count``."""
    if count < 1:
        raise SweepError("need at least one finalist")
    unique: list[Screen] = []
    seen: set[tuple[float, float]] = set()
    for s in sorted(frontier, key=lambda s: (s.nps, s.val_loss)):
        point = (s.val_loss, s.nps)
        if point not in seen:
            seen.add(point)
            unique.append(s)
    if len(unique) <= count:
        return unique
    if count == 1:
        # The single-finalist case has no span to sample; take the lowest-loss point.
        return [min(unique, key=lambda s: (s.val_loss, -s.nps))]
    picks = sorted({round(i * (len(unique) - 1) / (count - 1)) for i in range(count)})
    return [unique[i] for i in picks]


# --------------------------------------------------------------------------- #
# SPRT hand-off
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class SprtConfig:
    """The fixed-time-control SPRT contract every finalist is played under, against
    the current default network. ``elo0``/``elo1`` bound the hypothesis (the default
    tests "is the candidate a real gain": H0 no better, H1 at least ``elo1`` Elo);
    ``build_settings`` records the exact optimized build both sides run."""

    engine: str
    baseline_net: str
    baseline_id: str
    build_settings: str
    limit: str = "tc=10+0.1"
    elo0: float = 0.0
    elo1: float = 5.0
    max_games: int = 40_000
    strength_test: str = "tools/strength/strength_test.py"
    python: str = "python3"


def sprt_command(screen: Screen, sprt: SprtConfig, output_dir: str) -> str:
    """The exact ``strength_test.py`` invocation that plays one finalist against the
    default network: the same engine binary on both sides, told apart only by the
    per-side ``EvalFile`` (default net vs the finalist's exported network), under the
    fixed time control. Returned as a shell-ready string so the campaign can run it
    verbatim."""
    argv = [
        sprt.python, sprt.strength_test,
        "--baseline", sprt.engine, "--baseline-id", sprt.baseline_id,
        "--candidate", sprt.engine,
        "--candidate-id", f"{screen.name}:param_hash={screen.param_hash}",
        "--build-settings", sprt.build_settings,
        "--baseline-option", f"EvalFile={sprt.baseline_net}",
        "--candidate-option", f"EvalFile={screen.net_path}",
        "--limit", sprt.limit,
        "--elo0", str(sprt.elo0), "--elo1", str(sprt.elo1),
        "--max-games", str(sprt.max_games),
        "--output", f"{output_dir}/{screen.name}",
    ]
    return shlex.join(argv)


# --------------------------------------------------------------------------- #
# Orchestration
# --------------------------------------------------------------------------- #


@dataclass
class TrainingConfig:
    """The non-architectural configuration held fixed across every candidate — the
    fixed-everything-but-architecture rule the screen depends on. These flow into
    every training run unchanged; only the :class:`Architecture` differs."""

    corpus: Path
    manifest: Path
    epochs: int = 30
    batch_size: int = 8192
    lr: float = 1e-2
    lam: float = 0.3
    scale: int = 400
    qb: int = 64
    val_fraction: float = 0.1
    split_seed: int = 0
    seed: int = 0
    generation: int = 0
    device: str = "cpu"
    # Decode workers per candidate. Fixed across the sweep like every other
    # training knob; it only changes wall time, never the trained network.
    num_workers: int = 1


@dataclass
class SweepConfig:
    """A whole sweep run: what to train on, the candidate grid, where to write
    artifacts, the NPS-measurement engine, and the SPRT contract for the finalists."""

    training: TrainingConfig
    sprt: SprtConfig
    out_dir: Path
    grid: SweepGrid = field(default_factory=SweepGrid)
    finalists: int = 3
    binary_commit: Optional[str] = None


# A trainer runs one candidate and returns (post-QAT validation loss, exported SBNN
# path, parameter hash), or None if training/export failed for that candidate.
TrainAndExport = Callable[[Candidate], Optional[tuple[float, Path, str]]]
# An NPS measurer returns realized single-thread nodes-per-second for a network file.
MeasureNps = Callable[[Path], float]


def run_sweep(
    config: SweepConfig,
    train_and_export: TrainAndExport,
    measure_nps: MeasureNps,
    *,
    log=print,
) -> dict:
    """Screen every candidate, take the frontier, pick finalists, and return (and
    write) the machine-readable report with their SPRT commands.

    ``train_and_export`` and ``measure_nps`` are injected so the orchestration is
    testable without a GPU or the engine; the CLI passes the real implementations. A
    candidate whose training returns ``None`` is logged and dropped rather than
    aborting the sweep, so one bad point does not lose the rest."""
    screens: list[Screen] = []
    for candidate in enumerate_candidates(config.grid):
        outcome = train_and_export(candidate)
        if outcome is None:
            log(f"skip {candidate.name}: training or export failed")
            continue
        val_loss, net_path, param_hash = outcome
        nps = measure_nps(net_path)
        screen = Screen(
            candidate=candidate,
            val_loss=float(val_loss),
            nps=float(nps),
            param_hash=param_hash,
            net_path=str(net_path),
            binary_commit=config.binary_commit,
        )
        screens.append(screen)
        log(f"screened {candidate.name}: loss {val_loss:.6f}  nps {nps:,.0f}")

    frontier = pareto_frontier(screens)
    finalists = select_finalists(frontier, config.finalists)
    sprt_out = f"{config.out_dir}/sprt"
    commands = [sprt_command(s, config.sprt, sprt_out) for s in finalists]

    report = _build_report(config, screens, frontier, finalists, commands)
    config.out_dir.mkdir(parents=True, exist_ok=True)
    (config.out_dir / "sweep.json").write_text(json.dumps(report, indent=2) + "\n")
    log(
        f"screened {len(screens)} candidates, {len(frontier)} on the frontier, "
        f"{len(finalists)} finalists"
    )
    return report


def _screen_record(screen: Screen) -> dict:
    return {
        "name": screen.name,
        "factor": screen.candidate.factor,
        "architecture": asdict(screen.candidate.arch),
        "val_loss": screen.val_loss,
        "nps": screen.nps,
        "param_hash": screen.param_hash,
        "net_path": screen.net_path,
        "binary_commit": screen.binary_commit,
    }


def _build_report(
    config: SweepConfig,
    screens: list[Screen],
    frontier: list[Screen],
    finalists: list[Screen],
    commands: list[str],
) -> dict:
    """The machine-readable sweep result: the fixed training config, every screened
    point with attribution, the frontier and finalists, and the SPRT commands. This
    is the artifact the downstream campaign consumes to run the game matches."""
    return {
        "schema_version": 1,
        "training": {k: str(v) if isinstance(v, Path) else v
                     for k, v in asdict(config.training).items()},
        "binary_commit": config.binary_commit,
        "candidates": [_screen_record(s) for s in screens],
        "frontier": [s.name for s in frontier],
        "finalists": [s.name for s in finalists],
        "sprt_commands": commands,
    }


# --------------------------------------------------------------------------- #
# Production runners wired by the CLI (exercised on the rig, not in unit tests)
# --------------------------------------------------------------------------- #

_VAL_LOSS = re.compile(r"val_loss\s+([-+0-9.eE]+)")


def _default_train_and_export(config: SweepConfig, log=print) -> TrainAndExport:
    """A trainer that shells out to ``train.py`` (with the fixed config and the
    candidate's architecture, on the leak-free by-shard split) and ``export.py``,
    returning the final by-shard validation loss and the exported network's parameter
    hash. The subprocess boundary keeps torch out of this module."""
    here = Path(__file__).resolve().parent
    training = config.training

    def run(candidate: Candidate) -> Optional[tuple[float, Path, str]]:
        slug = candidate.name
        checkpoint = config.out_dir / "checkpoints" / f"{slug}.pt"
        net_path = config.out_dir / "nets" / f"{slug}.sbnn"
        checkpoint.parent.mkdir(parents=True, exist_ok=True)
        net_path.parent.mkdir(parents=True, exist_ok=True)

        train_cmd = [
            sys.executable, str(here / "train.py"),
            "--data", str(training.corpus),
            "--split", "by-shard", "--manifest", str(training.manifest),
            "--split-seed", str(training.split_seed),
            "--val-fraction", str(training.val_fraction),
            "--epochs", str(training.epochs), "--batch-size", str(training.batch_size),
            "--lr", str(training.lr), "--lambda", str(training.lam),
            "--scale", str(training.scale), "--seed", str(training.seed),
            "--generation", str(training.generation), "--device", training.device,
            "--num-workers", str(training.num_workers),
            "--out", str(checkpoint),
            *candidate.arch.train_flags(),
        ]
        completed = subprocess.run(train_cmd, capture_output=True, text=True, check=False)
        if completed.returncode != 0:
            log(f"train.py failed for {slug}: {completed.stderr.strip()[-500:]}")
            return None
        matches = _VAL_LOSS.findall(completed.stdout)
        if not matches:
            log(f"train.py printed no val_loss for {slug}")
            return None
        val_loss = float(matches[-1])

        export_cmd = [
            sys.executable, str(here / "export.py"),
            "--checkpoint", str(checkpoint), "--out", str(net_path),
        ]
        exported = subprocess.run(export_cmd, capture_output=True, text=True, check=False)
        if exported.returncode != 0:
            log(f"export.py failed for {slug}: {exported.stderr.strip()[-500:]}")
            return None
        return val_loss, net_path, read_param_hash(net_path)

    return run


def read_param_hash(net_path: Path) -> str:
    """The exported network's parameter hash, read straight from the SBNN header —
    the field the engine prints as its evaluator identity, so a screened point is
    attributable to exactly the network the SPRT match will load."""
    with open(net_path, "rb") as handle:
        handle.seek(_SBNN_PARAM_HASH_OFFSET)
        raw = handle.read(_SBNN_PARAM_HASH_BYTES)
    if len(raw) < _SBNN_PARAM_HASH_BYTES:
        raise SweepError(f"{net_path}: too short to hold an SBNN header")
    return f"0x{int.from_bytes(raw, 'little'):016x}"


_UCI_INFO = re.compile(r"\bnodes\s+(\d+)\b.*?\btime\s+(\d+)\b")


def measure_nps_uci(
    engine: Path, net_path: Path, suite: list[str], *, depth: int, hash_mb: int, log=print
) -> float:
    """Realized single-thread NPS: load ``net_path`` into the engine, search each
    suite position to a fixed depth on one thread, and aggregate total nodes over
    total search time. This is the in-engine incremental-accumulator cost the search
    actually pays — the methodology's cost axis — not a from-scratch forward-pass
    microbenchmark. Run on one quiet machine with an optimized ``target-cpu=native``
    build for the whole sweep, or the points are not comparable."""
    proc = subprocess.Popen(
        [str(engine)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, text=True,
    )
    assert proc.stdin and proc.stdout
    total_nodes = 0
    total_time_ms = 0
    try:
        proc.stdin.write(
            f"uci\nsetoption name Threads value 1\nsetoption name Hash value {hash_mb}\n"
            f"setoption name EvalFile value {net_path}\nisready\n"
        )
        proc.stdin.flush()
        for fen in suite:
            proc.stdin.write(f"ucinewgame\nposition fen {fen}\ngo depth {depth}\n")
            proc.stdin.flush()
            nodes = time_ms = 0
            while True:
                line = proc.stdout.readline()
                if not line:
                    raise SweepError("engine closed its output mid-search")
                if line.startswith("info"):
                    match = _UCI_INFO.search(line)
                    if match:
                        nodes, time_ms = int(match.group(1)), int(match.group(2))
                elif line.startswith("bestmove"):
                    break
            total_nodes += nodes
            total_time_ms += time_ms
        proc.stdin.write("quit\n")
        proc.stdin.flush()
    finally:
        proc.terminate()
    if total_time_ms == 0:
        raise SweepError("suite search reported zero elapsed time")
    return total_nodes / (total_time_ms / 1000.0)


def _load_suite(path: Path) -> list[str]:
    """FENs (or EPDs) from a suite file, one per line, blanks and ``#`` comments
    skipped. Only the position is needed; any trailing EPD operations are ignored."""
    fens: list[str] = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fens.append(line)
    return fens


def _git_commit(source_dir: Optional[Path]) -> Optional[str]:
    if source_dir is None:
        return None
    try:
        out = subprocess.run(
            ["git", "-C", str(source_dir), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        )
    except (subprocess.CalledProcessError, OSError):
        return None
    return out.stdout.strip() or None


def _parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--engine", type=Path, required=True, help="built seaborg release binary")
    p.add_argument("--corpus", type=Path, required=True, help="packed corpus (corpus.bin)")
    p.add_argument("--manifest", type=Path, default=None,
                   help="corpus provenance manifest; defaults to beside the corpus")
    p.add_argument("--baseline-net", type=Path, required=True,
                   help="the current default network the finalists are tested against")
    p.add_argument("--baseline-id", required=True, help="immutable identity of the default network")
    p.add_argument("--out-dir", type=Path, required=True, help="directory for artifacts and report")
    p.add_argument("--nps-suite", type=Path, required=True,
                   help="fixed FEN/EPD position suite for the NPS measurement")
    p.add_argument("--nps-depth", type=int, default=13, help="fixed search depth for NPS")
    p.add_argument("--hash-mb", type=int, default=64)
    p.add_argument("--build-settings", required=True,
                   help="exact optimized build command/flags/target, recorded with each result")
    p.add_argument("--finalists", type=int, default=3)
    p.add_argument("--epochs", type=int, default=30)
    p.add_argument("--batch-size", type=int, default=8192)
    p.add_argument("--lr", type=float, default=1e-2)
    p.add_argument("--lambda", dest="lam", type=float, default=0.3)
    p.add_argument("--scale", type=int, default=400)
    p.add_argument("--val-fraction", type=float, default=0.1)
    p.add_argument("--split-seed", type=int, default=0)
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--device", default="cpu")
    p.add_argument(
        "--num-workers", type=int, default=min(8, os.cpu_count() or 1),
        help="decode workers per candidate training run; set to the host's physical "
        "core count. Fixed across the sweep; changes only wall time, not the nets.",
    )
    p.add_argument("--limit", default="tc=10+0.1", help="SPRT time control for the finalists")
    p.add_argument("--elo0", type=float, default=0.0)
    p.add_argument("--elo1", type=float, default=5.0)
    p.add_argument("--max-games", type=int, default=40_000)
    p.add_argument("--source-dir", type=Path, default=None,
                   help="seaborg checkout the binary was built from, for the commit record")
    return p


def main(argv: Optional[list[str]] = None) -> int:
    args = _parser().parse_args(argv)
    manifest = args.manifest if args.manifest is not None else args.corpus.parent
    commit = _git_commit(args.source_dir)
    config = SweepConfig(
        training=TrainingConfig(
            corpus=args.corpus, manifest=manifest, epochs=args.epochs,
            batch_size=args.batch_size, lr=args.lr, lam=args.lam, scale=args.scale,
            val_fraction=args.val_fraction, split_seed=args.split_seed, seed=args.seed,
            device=args.device, num_workers=args.num_workers,
        ),
        sprt=SprtConfig(
            engine=str(args.engine), baseline_net=str(args.baseline_net),
            baseline_id=args.baseline_id, build_settings=args.build_settings,
            limit=args.limit, elo0=args.elo0, elo1=args.elo1, max_games=args.max_games,
        ),
        out_dir=args.out_dir,
        finalists=args.finalists,
        binary_commit=commit,
    )
    suite = _load_suite(args.nps_suite)

    def measure(net_path: Path) -> float:
        return measure_nps_uci(
            args.engine, net_path, suite, depth=args.nps_depth, hash_mb=args.hash_mb
        )

    run_sweep(config, _default_train_and_export(config), measure)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
