"""Deterministic by-shard (by-game) train/validation split for the packed corpus.

The obvious split — shuffle every position and take a fraction for validation —
leaks. Positions within one self-play game are near-duplicates: successive plies
differ by a single move, share almost all material and pawn structure, and carry
the *same* game-outcome label. A by-position split therefore puts near-twins on
both sides of the boundary, making validation loss optimistic and, worse for an
architecture sweep, compressing the loss gap between candidates the screen must
resolve.

The packed record (``engine::selfplay::format``) stores no game id, so the split
is defined at the granularity the corpus actually records: whole datagen *runs*
(shards). A shard is a self-contained ``seaborg datagen`` stream whose positions
are whole games, so reserving whole shards for validation guarantees no game — and
no position's same-game neighbour — straddles the boundary. ``concat_samples``
joins shards in manifest order, so each shard occupies one contiguous record span
in the corpus, which :func:`shard_spans` reconstructs from ``corpus.manifest.json``.

Which shards are held out is a fixed function of shard identity: each shard is
hashed by its ``(opening_seed, file)`` run identity folded with a split seed, the
shards are ranked by that hash, and the lowest-hash whole shards are reserved for
validation until their cumulative record count first reaches ``val_fraction`` of
the corpus (never emptying the training side). The hash makes the split
reproducible byte-for-byte for a given corpus and seed, so every candidate in a
sweep trains and validates on the identical partition — the confound the
fixed-everything-else screen exists to remove.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np

# The provenance manifest ``datagen_campaign`` writes beside ``corpus.bin``. It
# lists every shard in join order with its record count, which is all the split
# needs to map a shard to its contiguous record span in the joined corpus.
MANIFEST_NAME = "corpus.manifest.json"


class SplitError(ValueError):
    """The manifest and corpus disagree, or the requested split is degenerate."""


@dataclass(frozen=True)
class ShardSpan:
    """One shard's contiguous record span in the joined corpus. ``name`` and
    ``opening_seed`` are the shard's run identity (both stable and unique per run);
    ``[start, stop)`` are its record indices in the corpus, in join order."""

    name: str
    opening_seed: int
    start: int
    stop: int

    @property
    def records(self) -> int:
        return self.stop - self.start


@dataclass(frozen=True)
class Split:
    """A by-shard partition of the corpus. ``train_idx`` and ``val_idx`` are the
    disjoint record indices; ``val_shards`` and ``train_shards`` name the shards on
    each side, so the split is auditable against the manifest."""

    train_idx: np.ndarray
    val_idx: np.ndarray
    val_shards: tuple[str, ...]
    train_shards: tuple[str, ...]


def load_manifest(path) -> dict:
    """Read a corpus provenance manifest. ``path`` may be the manifest file itself
    or the directory holding it beside ``corpus.bin``."""
    path = Path(path)
    if path.is_dir():
        path = path / MANIFEST_NAME
    return json.loads(path.read_text())


def shard_spans(manifest: dict) -> list[ShardSpan]:
    """Reconstruct each shard's contiguous record span from the manifest, in the
    join order ``concat_samples`` wrote. The spans tile ``[0, total_records)`` with
    no gap or overlap, so a whole-shard holdout cannot split a game."""
    shards = manifest.get("shards")
    if not shards:
        raise SplitError("manifest lists no shards")
    spans: list[ShardSpan] = []
    cursor = 0
    for entry in shards:
        records = int(entry["records"])
        if records <= 0:
            raise SplitError(f"shard {entry.get('file')!r} records {records} is not positive")
        spans.append(
            ShardSpan(
                name=str(entry["file"]),
                opening_seed=int(entry["opening_seed"]),
                start=cursor,
                stop=cursor + records,
            )
        )
        cursor += records
    declared = manifest.get("total_records")
    if declared is not None and int(declared) != cursor:
        raise SplitError(
            f"manifest total_records {declared} disagrees with the shard record sum {cursor}"
        )
    return spans


def _shard_hash(span: ShardSpan, seed: int) -> int:
    """A deterministic 64-bit hash of a shard's run identity, folded with the split
    seed. Uses BLAKE2b rather than the salted built-in ``hash`` so the value — and
    thus the whole split — is identical across processes and machines."""
    key = f"{seed}:{span.opening_seed}:{span.name}".encode()
    return int.from_bytes(hashlib.blake2b(key, digest_size=8).digest(), "big")


def by_shard_split(manifest: dict, *, val_fraction: float, seed: int = 0) -> Split:
    """Partition the corpus into train/validation index arrays by reserving whole
    lowest-hash shards for validation until they first cover ``val_fraction`` of the
    records, leaving at least one shard on each side.

    The result is a pure function of the manifest and ``seed``: the same inputs
    yield byte-identical index arrays every call, so a sweep reuses one split across
    every candidate. Raises :class:`SplitError` if the corpus has fewer than two
    shards or the fraction cannot carve out a non-empty validation set without
    emptying training."""
    if not 0.0 < val_fraction < 1.0:
        raise SplitError(f"val_fraction must be in (0, 1), got {val_fraction}")
    spans = shard_spans(manifest)
    if len(spans) < 2:
        raise SplitError(
            f"a by-shard split needs at least two shards to hold one out; got {len(spans)}"
        )
    total = spans[-1].stop
    target = val_fraction * total

    # Rank by the identity hash (name breaks the astronomically unlikely tie so the
    # order is total and deterministic), then take whole shards for validation until
    # they first reach the target — but never the last one, so training keeps a shard.
    ranked = sorted(spans, key=lambda s: (_shard_hash(s, seed), s.name))
    val_names: set[str] = set()
    covered = 0
    for span in ranked[:-1]:
        if covered >= target:
            break
        val_names.add(span.name)
        covered += span.records

    val_spans = [s for s in spans if s.name in val_names]
    train_spans = [s for s in spans if s.name not in val_names]
    if not val_spans or not train_spans:
        # Only reachable if the arithmetic above is wrong; kept as a guard so a
        # degenerate split can never silently reach the trainer.
        raise SplitError("by-shard split left one side empty")

    val_idx = _spans_to_indices(val_spans)
    train_idx = _spans_to_indices(train_spans)
    return Split(
        train_idx=train_idx,
        val_idx=val_idx,
        # Report shards in join order, not hash order, so the record reads naturally.
        val_shards=tuple(s.name for s in val_spans),
        train_shards=tuple(s.name for s in train_spans),
    )


def _spans_to_indices(spans: list[ShardSpan]) -> np.ndarray:
    """The concatenated record indices of ``spans``, ascending. Whole contiguous
    ranges, so no index is ever shared with a shard on the other side."""
    if not spans:
        return np.empty(0, dtype=np.int64)
    return np.concatenate([np.arange(s.start, s.stop, dtype=np.int64) for s in spans])
