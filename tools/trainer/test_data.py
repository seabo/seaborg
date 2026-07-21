"""Tests for the packed-format dataloader: feature-index correctness against the
contract's formula, side-to-move selection, target decoding, and stream
validation. NumPy only -- no PyTorch dependency."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import numpy as np

import data
from testsupport import (
    BLACK_KING,
    WHITE_KING,
    WHITE_PAWN,
    encode_record,
    encode_stream,
    mirror,
)


class FeatureIndexTest(unittest.TestCase):
    def test_indices_match_the_contract_formula(self):
        # White pawn on a1 (sq 0), white king on e1 (sq 4), black king on e8
        # (sq 60); White to move. Indices below are computed by hand from
        # index = oriented + 64*pt0 + 384*side.
        pieces = {0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING}
        batch = data.decode(np.stack([encode_record(pieces)]))

        # Ascending square order: pawn (sq0), white king (sq4), black king (sq60).
        # White is the side to move, so stm == white perspective.
        self.assertEqual(list(batch.stm_indices), [0, 324, 764])
        self.assertEqual(list(batch.nstm_indices), [440, 764, 324])
        self.assertEqual(list(batch.offsets), [0])

    def test_side_to_move_selects_the_perspective(self):
        pieces = {0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING}
        white = data.decode(np.stack([encode_record(pieces, black_to_move=False)]))
        black = data.decode(np.stack([encode_record(pieces, black_to_move=True)]))
        # Flipping the side to move swaps which perspective is stm vs nstm.
        self.assertEqual(list(black.stm_indices), list(white.nstm_indices))
        self.assertEqual(list(black.nstm_indices), list(white.stm_indices))

    def test_offsets_and_counts_track_piece_totals(self):
        one = {4: WHITE_KING, 60: BLACK_KING}
        two = {0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING}
        batch = data.decode(np.stack([encode_record(one), encode_record(two)]))
        self.assertEqual(list(batch.offsets), [0, 2])  # second sample starts after 2 features
        self.assertEqual(len(batch.stm_indices), 5)  # 2 + 3 pieces
        self.assertEqual(len(batch), 2)

    def test_all_indices_are_in_range(self):
        # Every piece type on a spread of squares stays within 0..768.
        pieces = {sq: (sq % 12) + 1 for sq in range(0, 64, 3)}
        batch = data.decode(np.stack([encode_record(pieces)]))
        self.assertTrue((batch.stm_indices >= 0).all())
        self.assertTrue((batch.stm_indices < 768).all())
        self.assertTrue((batch.nstm_indices >= 0).all())
        self.assertTrue((batch.nstm_indices < 768).all())


class TargetDecodeTest(unittest.TestCase):
    def test_score_round_trips_including_negatives(self):
        for score in (0, 37, -123, 456, 20000, -20000):
            batch = data.decode(
                np.stack([encode_record({4: WHITE_KING, 60: BLACK_KING}, score=score)])
            )
            self.assertEqual(int(batch.score[0]), score)

    def test_wdl_bytes_pass_through(self):
        for wdl in (0, 1, 2):
            batch = data.decode(
                np.stack([encode_record({4: WHITE_KING, 60: BLACK_KING}, wdl=wdl)])
            )
            self.assertEqual(int(batch.wdl[0]), wdl)


class StreamValidationTest(unittest.TestCase):
    def _write(self, blob: bytes) -> Path:
        handle = tempfile.NamedTemporaryFile(suffix=".bin", delete=False)
        handle.write(blob)
        handle.close()
        path = Path(handle.name)
        self.addCleanup(path.unlink)
        return path

    def test_reads_a_written_stream(self):
        records = [
            encode_record({4: WHITE_KING, 60: BLACK_KING}, score=10, wdl=2),
            encode_record({0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING}, score=-5, wdl=0),
        ]
        path = self._write(encode_stream(records))
        packed = data.PackedData(path)
        self.assertEqual(len(packed), 2)
        batch = packed.batch(np.array([0, 1]))
        self.assertEqual(list(batch.score), [10, -5])
        self.assertEqual(list(batch.wdl), [2, 0])

    def test_rejects_bad_magic(self):
        blob = bytearray(encode_stream([encode_record({4: WHITE_KING, 60: BLACK_KING})]))
        blob[0:4] = b"XXXX"
        with self.assertRaises(data.FormatError):
            data.PackedData(self._write(bytes(blob)))

    def test_rejects_unsupported_version(self):
        blob = bytearray(encode_stream([encode_record({4: WHITE_KING, 60: BLACK_KING})]))
        blob[4:6] = (data.FORMAT_VERSION + 1).to_bytes(2, "little")
        with self.assertRaises(data.FormatError):
            data.PackedData(self._write(bytes(blob)))

    def test_rejects_truncated_record(self):
        blob = encode_stream([encode_record({4: WHITE_KING, 60: BLACK_KING})])
        with self.assertRaises(data.FormatError):
            data.PackedData(self._write(blob[:-1]))


class MirrorInvarianceTest(unittest.TestCase):
    def test_mirror_produces_the_same_stm_features(self):
        # A position and its colour-and-board-flipped mirror (with the side to
        # move flipped too) present the identical concatenated input to the
        # network, because both are read from the mover's perspective. This is
        # the structural basis of the "equal and opposite evaluation" property.
        pieces = {0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING, 20: WHITE_PAWN}
        original = data.decode(np.stack([encode_record(pieces, black_to_move=False)]))
        flipped = data.decode(np.stack([encode_record(mirror(pieces), black_to_move=True)]))
        self.assertEqual(sorted(original.stm_indices), sorted(flipped.stm_indices))
        self.assertEqual(sorted(original.nstm_indices), sorted(flipped.nstm_indices))


if __name__ == "__main__":
    unittest.main()
