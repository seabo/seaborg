"""Chain many self-play datagen shards into one accumulating corpus.

The bootstrap generated one sample file per generation. Growing a substantially
larger corpus for a bigger network means running datagen many times and joining
the results, and it wants two things the single-shot path did not:

* a *distinct opening seed per shard*, so the corpus is diversified across its
  whole length rather than replaying one opening book every shard (a gotcha the
  bootstrap loop hit -- it reused one seed across generations);
* a recorded provenance manifest -- the labelling network's hash and id, the
  binary commit, and every generation parameter -- so the finished corpus is
  reproducible and attributable long after the run.

Each shard is a self-contained ``seaborg datagen --out`` stream. The campaign
runs shards until an accumulated-sample target is reached (or a fixed shard
count), then joins them into one corpus with :mod:`concat_samples`. Shards are
kept, not deleted: the corpus is usable and the campaign extendable at any
checkpoint, and a crashed shard is isolated rather than corrupting the whole.

Purity: the only evaluator is the network passed as ``--network`` playing
seaborg against itself. No external engine, opening database, evaluation, or
tablebase is ever an input, which the manifest states explicitly.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Optional

import concat_samples

# Default per-move label budget for this campaign: five times the bootstrap's
# 5000 nodes. A larger future network can exploit deeper, quieter labels, and
# under-budgeting is the one parameter that would force regenerating the whole
# corpus, so it is set generously high. Kept as a node budget (not a depth
# limit) so it stays directly comparable to the bootstrap and reproducible under
# the existing throughput calibration.
DEFAULT_NODES = 25_000


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def network_id(path: Path, generation: int) -> str:
    """The labelling network's stable id, in the reinforcement loop's format so
    the corpus is attributable to the same network identity the loop records."""
    return f"nnue:gen-{generation:03d}:sha256={sha256(path)[:16]}"


def git_commit(source_dir: Optional[Path]) -> Optional[str]:
    """The commit the binary was built from, read from the source checkout, or
    ``None`` if it cannot be determined. Recorded so the run is reproducible."""
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


@dataclass
class CampaignConfig:
    engine: Path
    network: Path
    network_generation: int
    out_dir: Path
    nodes: int = DEFAULT_NODES
    games_per_shard: int = 60_000
    workers: int = 12
    base_seed: int = 100_000
    opening_plies: int = 6
    filter_opening_plies: int = 8
    target_samples: Optional[int] = None
    shards: Optional[int] = None
    datagen_extra: list = field(default_factory=list)
    source_dir: Optional[Path] = None


def shard_path(out_dir: Path, index: int) -> Path:
    return out_dir / f"shard_{index:03d}.bin"


def datagen_command(config: CampaignConfig, seed: int, out: Path) -> list:
    """The ``seaborg datagen`` invocation for one shard. The evaluator is named
    explicitly with ``--network`` -- self-play purity depends on the labeller
    being exactly this network, never whatever the binary might embed."""
    return [
        str(config.engine), "datagen",
        "--network", str(config.network),
        "--nodes", str(config.nodes),
        "--games", str(config.games_per_shard),
        "--workers", str(config.workers),
        "--opening-seed", str(seed),
        "--opening-plies", str(config.opening_plies),
        "--filter-opening-plies", str(config.filter_opening_plies),
        "--out", str(out),
        *config.datagen_extra,
    ]


def _reached_target(config: CampaignConfig, produced_shards: int, total_samples: int) -> bool:
    """Stop once either bound is met: the sample target or the shard count.
    With neither set, the campaign has no end and the caller must bound it."""
    if config.shards is not None and produced_shards >= config.shards:
        return True
    if config.target_samples is not None and total_samples >= config.target_samples:
        return True
    return False


def build_manifest(config: CampaignConfig, shards: list, total: int) -> dict:
    return {
        "corpus": "corpus.bin",
        "labeller": {
            "network": str(config.network),
            "network_id": network_id(config.network, config.network_generation),
            "sha256": sha256(config.network),
        },
        "binary": {
            "commit": git_commit(config.source_dir),
            "engine": str(config.engine),
        },
        "purity": (
            "seaborg self-play labelled solely by the network above; no external "
            "engine, opening database, evaluation, or tablebase entered the corpus"
        ),
        "params": {
            "nodes": config.nodes,
            "games_per_shard": config.games_per_shard,
            "workers": config.workers,
            "base_seed": config.base_seed,
            "opening_plies": config.opening_plies,
            "filter_opening_plies": config.filter_opening_plies,
            "datagen_extra": list(config.datagen_extra),
        },
        "shards": shards,
        "total_records": total,
    }


def run(
    config: CampaignConfig,
    runner: Callable[[list, Path], int],
    *,
    log=print,
) -> dict:
    """Generate shards until a stop bound, join them into ``out_dir/corpus.bin``,
    and return the provenance manifest (also written beside the corpus).

    ``runner(command, out_path)`` executes one datagen shard and returns its exit
    code; it is injected so the chaining logic is testable without running the
    engine. A non-zero exit aborts the campaign with the shards produced so far
    left in place.
    """
    if config.target_samples is None and config.shards is None:
        raise ValueError("set --target-samples or --shards; the campaign needs an end")

    config.out_dir.mkdir(parents=True, exist_ok=True)
    shards: list = []
    total = 0
    index = 0
    while not _reached_target(config, index, total):
        seed = config.base_seed + index
        out = shard_path(config.out_dir, index)
        code = runner(datagen_command(config, seed, out), out)
        if code != 0:
            raise RuntimeError(f"datagen shard {index} exited {code}; {len(shards)} shards kept")
        records = concat_samples.shard_records(out)
        shards.append({"file": out.name, "opening_seed": seed, "records": records})
        total += records
        index += 1
        log(f"shard {index}: {records} records (total {total})")

    corpus = config.out_dir / "corpus.bin"
    joined = concat_samples.concat([config.out_dir / s["file"] for s in shards], corpus)
    concat_samples.verify(corpus, joined)

    manifest = build_manifest(config, shards, joined)
    (config.out_dir / "corpus.manifest.json").write_text(json.dumps(manifest, indent=2))
    log(f"corpus.bin: {joined} records from {len(shards)} shards")
    return manifest


def _subprocess_runner(out_dir: Path) -> Callable[[list, Path], int]:
    def runner(command: list, out: Path) -> int:
        log_path = out.with_suffix(".datagen.log")
        with open(log_path, "wb") as handle:
            handle.write((" ".join(command) + "\n\n").encode())
            handle.flush()
            completed = subprocess.run(command, stdout=handle, stderr=subprocess.STDOUT, check=False)
        return completed.returncode
    return runner


def config_from_args(args: argparse.Namespace) -> CampaignConfig:
    return CampaignConfig(
        engine=args.engine,
        network=args.network,
        network_generation=args.network_generation,
        out_dir=args.out_dir,
        nodes=args.nodes,
        games_per_shard=args.games_per_shard,
        workers=args.workers,
        base_seed=args.base_seed,
        opening_plies=args.opening_plies,
        filter_opening_plies=args.filter_opening_plies,
        target_samples=args.target_samples,
        shards=args.shards,
        datagen_extra=args.datagen_arg,
        source_dir=args.source_dir,
    )


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--engine", type=Path, required=True, help="built seaborg release binary")
    p.add_argument("--network", type=Path, required=True, help="labelling network (.sbnn)")
    p.add_argument("--network-generation", type=int, default=2,
                   help="generation number of the labelling network, for its id")
    p.add_argument("--out-dir", type=Path, required=True, help="directory for shards and corpus")
    p.add_argument("--nodes", type=int, default=DEFAULT_NODES,
                   help="per-move label search budget (higher than the bootstrap's 5000)")
    p.add_argument("--games-per-shard", type=int, default=60_000)
    p.add_argument("--workers", type=int, default=12,
                   help="datagen workers; peaks at the physical core count")
    p.add_argument("--base-seed", type=int, default=100_000,
                   help="opening seed of shard 0; each shard adds its index")
    p.add_argument("--opening-plies", type=int, default=6)
    p.add_argument("--filter-opening-plies", type=int, default=8)
    p.add_argument("--target-samples", type=int, default=None,
                   help="stop once the accumulated corpus reaches this many samples")
    p.add_argument("--shards", type=int, default=None, help="stop after this many shards")
    p.add_argument("--source-dir", type=Path, default=None,
                   help="seaborg checkout the binary was built from, for the commit record")
    p.add_argument("--datagen-arg", action="append", default=[], metavar="ARG",
                   help="extra argument forwarded verbatim to seaborg datagen (repeatable)")
    return p


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    config = config_from_args(args)
    run(config, _subprocess_runner(config.out_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
