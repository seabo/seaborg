"""Tests for the shard-accumulation helper: that joining is header-aware and
byte-exact, that a mismatched or truncated shard is rejected before any output
is written, and that the joined corpus is a valid single stream the training
dataloader reads back with the expected record count."""

from __future__ import annotations

import struct
import sys
import tempfile
import unittest
from pathlib import Path

import concat_samples as cs

# The dataloader lives beside the trainer; import it for the round-trip check but
# skip that check gracefully where NumPy is not installed (the helper itself has
# no third-party dependency).
_TRAINER = Path(__file__).resolve().parents[1] / "trainer"
sys.path.insert(0, str(_TRAINER))
try:
    import data as trainer_data  # type: ignore
except ImportError:  # pragma: no cover - exercised only without NumPy
    trainer_data = None


def _record(fill: int) -> bytes:
    """A 32-byte record; contents are arbitrary since joining never decodes."""
    return bytes([fill % 256]) * cs.RECORD_SIZE


def _shard(records: list[bytes], *, version: int = cs.FORMAT_VERSION,
           record_size: int = cs.RECORD_SIZE) -> bytes:
    header = cs.MAGIC + struct.pack("<HH", version, record_size)
    return header + b"".join(records)


class ConcatTest(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())

    def _write(self, name: str, blob: bytes) -> Path:
        path = self.dir / name
        path.write_bytes(blob)
        return path

    def test_joins_bodies_under_one_header_in_order(self):
        a = self._write("a.bin", _shard([_record(1), _record(2)]))
        b = self._write("b.bin", _shard([_record(3)]))
        out = self.dir / "corpus.bin"

        total = cs.concat([a, b], out)

        self.assertEqual(total, 3)
        # Exactly one header, then a's two records then b's one, in that order.
        expected = _shard([_record(1), _record(2), _record(3)])
        self.assertEqual(out.read_bytes(), expected)

    def test_record_count_matches_body_size(self):
        shard = self._write("s.bin", _shard([_record(i) for i in range(5)]))
        self.assertEqual(cs.shard_records(shard), 5)

    def test_rejects_mismatched_record_size(self):
        a = self._write("a.bin", _shard([_record(1)]))
        b = self._write("b.bin", _shard([b"\x00" * 16], record_size=16))
        out = self.dir / "corpus.bin"
        with self.assertRaises(cs.FormatError):
            cs.concat([a, b], out)
        # Nothing is written when a shard is rejected during validation.
        self.assertFalse(out.exists())

    def test_rejects_unsupported_version(self):
        a = self._write("a.bin", _shard([_record(1)], version=cs.FORMAT_VERSION + 1))
        with self.assertRaises(cs.FormatError):
            cs.concat([a], self.dir / "corpus.bin")

    def test_rejects_bad_magic(self):
        a = self._write("a.bin", b"XXXX" + struct.pack("<HH", 1, 32) + _record(1))
        with self.assertRaises(cs.FormatError):
            cs.concat([a], self.dir / "corpus.bin")

    def test_rejects_body_not_a_whole_number_of_records(self):
        # Header plus 40 bytes: one record and a 8-byte tail.
        a = self._write("a.bin", _shard([_record(1)]) + b"\x00" * 8)
        with self.assertRaises(cs.FormatError):
            cs.shard_records(a)

    def test_rejects_no_inputs(self):
        with self.assertRaises(ValueError):
            cs.concat([], self.dir / "corpus.bin")

    def test_verify_accepts_correct_count_and_rejects_wrong(self):
        a = self._write("a.bin", _shard([_record(1), _record(2)]))
        out = self.dir / "corpus.bin"
        total = cs.concat([a], out)
        cs.verify(out, total)  # does not raise
        with self.assertRaises(cs.FormatError):
            cs.verify(out, total + 1)

    def test_joined_corpus_loads_in_the_dataloader(self):
        if trainer_data is None:
            self.skipTest("trainer dataloader (NumPy) not importable")
        # Build real records via the trainer's reference encoder so the join is
        # validated against the actual consumer, not just its byte length.
        sys.path.insert(0, str(_TRAINER))
        import testsupport  # type: ignore

        kings = {4: testsupport.WHITE_KING, 60: testsupport.BLACK_KING}
        r1 = bytes(testsupport.encode_record(kings, score=100, wdl=2))
        r2 = bytes(testsupport.encode_record(kings, score=-50, wdl=0))
        a = self._write("a.bin", _shard([r1]))
        b = self._write("b.bin", _shard([r2, r1]))
        out = self.dir / "corpus.bin"

        total = cs.concat([a, b], out)
        cs.verify(out, total)

        loaded = trainer_data.PackedData(out)
        self.assertEqual(len(loaded), 3)


if __name__ == "__main__":
    unittest.main()
