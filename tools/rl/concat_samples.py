"""Accumulate self-play sample shards into one packed corpus.

Each ``seaborg datagen --out`` run writes a self-contained stream: an 8-byte
header (magic ``SBRG``, a format version, the record size) followed by fixed
32-byte little-endian records (``engine::selfplay::format``). The training
dataloader (``tools/trainer/data.py``) memory-maps one file, so a campaign that
generates the corpus as many separate shards must join them into a single
stream before training.

Joining is header-aware, not a raw ``cat``: the output carries exactly one
header, then every shard's record *body* concatenated in order. Blindly
concatenating whole files would leave an 8-byte header buried mid-stream every
shard boundary, which the reader would misread as record bytes and shift the
whole corpus out of alignment. A shard whose header disagrees with the first
(different magic, version, or record size) or whose body is not a whole number
of records is rejected rather than silently corrupting the corpus.
"""

from __future__ import annotations

import argparse
from pathlib import Path

# The stream framing, mirroring the Rust writer (engine::selfplay::format) and
# the dataloader (tools/trainer/data.py). Defined locally, as loop.py does, so a
# format change is a deliberate edit here rather than a silent cross-tool import.
MAGIC = b"SBRG"
FORMAT_VERSION = 1
HEADER_SIZE = 8
RECORD_SIZE = 32

# Copy the record body in modest blocks so a multi-gigabyte shard never has to
# be read into memory whole. A multiple of RECORD_SIZE keeps the boundaries
# clean, though correctness does not depend on it since bodies are validated to
# be record-aligned first.
_COPY_CHUNK = RECORD_SIZE * 65536


class FormatError(ValueError):
    """A shard's header or body did not match the packed sample format."""


def read_header(path: Path) -> tuple[int, int]:
    """Return ``(version, record_size)`` from a shard's 8-byte header, rejecting
    a stream that is too short or does not carry the expected magic."""
    with open(path, "rb") as handle:
        header = handle.read(HEADER_SIZE)
    if len(header) < HEADER_SIZE:
        raise FormatError(f"{path}: shorter than the {HEADER_SIZE}-byte stream header")
    if header[0:4] != MAGIC:
        raise FormatError(f"{path}: not a Seaborg sample stream (bad magic)")
    version = int.from_bytes(header[4:6], "little")
    record_size = int.from_bytes(header[6:8], "little")
    return version, record_size


def shard_records(path: Path, record_size: int = RECORD_SIZE) -> int:
    """Number of records in a shard: its size past the header, in records. Raises
    if the body is not a whole number of records (a truncated or corrupt shard)."""
    body = path.stat().st_size - HEADER_SIZE
    if body < 0:
        raise FormatError(f"{path}: shorter than the {HEADER_SIZE}-byte stream header")
    if body % record_size != 0:
        raise FormatError(f"{path}: body of {body} bytes is not a multiple of {record_size}")
    return body // record_size


def concat(inputs: list[Path], output: Path) -> int:
    """Join ``inputs`` into one packed stream at ``output`` and return the total
    record count. All shards must share one header; the output writes it once,
    then every shard's record body in the given order."""
    if not inputs:
        raise ValueError("no input shards given")

    version, record_size = read_header(inputs[0])
    if version != FORMAT_VERSION:
        raise FormatError(f"{inputs[0]}: unsupported format version {version}")

    # Validate every shard up front so a bad shard aborts before any bytes are
    # written, rather than leaving a half-built corpus behind.
    total = 0
    for shard in inputs:
        shard_version, shard_record_size = read_header(shard)
        if (shard_version, shard_record_size) != (version, record_size):
            raise FormatError(
                f"{shard}: header (version {shard_version}, record size "
                f"{shard_record_size}) disagrees with {inputs[0]} (version "
                f"{version}, record size {record_size})"
            )
        total += shard_records(shard, record_size)

    header = MAGIC + version.to_bytes(2, "little") + record_size.to_bytes(2, "little")
    with open(output, "wb") as out:
        out.write(header)
        for shard in inputs:
            with open(shard, "rb") as src:
                src.seek(HEADER_SIZE)
                while True:
                    block = src.read(_COPY_CHUNK)
                    if not block:
                        break
                    out.write(block)
    return total


def verify(output: Path, expected_records: int) -> None:
    """Re-read the joined corpus and confirm its header is valid and it holds
    exactly ``expected_records`` records, so a completed join is trustworthy
    before hours of training depend on it."""
    version, record_size = read_header(output)
    if version != FORMAT_VERSION:
        raise FormatError(f"{output}: unsupported format version {version}")
    actual = shard_records(output, record_size)
    if actual != expected_records:
        raise FormatError(
            f"{output}: holds {actual} records, expected {expected_records}"
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True, help="corpus file to write")
    parser.add_argument("shards", type=Path, nargs="+", help="shard files to join, in order")
    parser.add_argument(
        "--verify",
        action="store_true",
        help="re-read the joined corpus and confirm its record count",
    )
    args = parser.parse_args(argv)

    total = concat(args.shards, args.out)
    if args.verify:
        verify(args.out, total)
    print(f"Joined {len(args.shards)} shards into {args.out}: {total} records")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
