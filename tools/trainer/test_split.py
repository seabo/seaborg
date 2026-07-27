"""Tests for the by-shard (by-game) validation split.

The split's whole reason to exist is leak-freedom: no game — and therefore no
position's same-game neighbour — may sit on both sides of the train/validation
boundary. Because a game is always a subset of one datagen run (shard), that
reduces to "no shard straddles the boundary", which these tests assert directly
on the record indices. They also pin the split's determinism: one corpus and seed
must yield the identical partition on every call, so a sweep compares candidates on
the same held-out games.
"""

from __future__ import annotations

import json
import unittest

import numpy as np

from split import (
    Split,
    SplitError,
    by_shard_split,
    load_manifest,
    shard_spans,
)


def _manifest(records: list[int], base_seed: int = 100_000) -> dict:
    """A manifest of shards with the given record counts, named and seeded the way
    ``datagen_campaign`` writes them (``shard_NNN.bin``, ``base_seed + index``)."""
    shards = [
        {"file": f"shard_{i:03d}.bin", "opening_seed": base_seed + i, "records": n}
        for i, n in enumerate(records)
    ]
    return {"shards": shards, "total_records": sum(records)}


class ShardSpanTest(unittest.TestCase):
    def test_spans_tile_the_corpus_without_gap_or_overlap(self):
        spans = shard_spans(_manifest([10, 20, 5]))
        self.assertEqual([(s.start, s.stop) for s in spans], [(0, 10), (10, 30), (30, 35)])
        self.assertEqual([s.records for s in spans], [10, 20, 5])

    def test_total_records_disagreement_is_rejected(self):
        manifest = _manifest([10, 20])
        manifest["total_records"] = 31
        with self.assertRaises(SplitError):
            shard_spans(manifest)

    def test_empty_manifest_is_rejected(self):
        with self.assertRaises(SplitError):
            shard_spans({"shards": []})


class BySplitLeakageTest(unittest.TestCase):
    def _assert_partition(self, split: Split, total: int) -> None:
        """The two index sets are disjoint, together cover every record exactly
        once, and each is a plain ascending run of indices."""
        train = split.train_idx
        val = split.val_idx
        self.assertEqual(train.size + val.size, total)
        union = np.concatenate([train, val])
        np.testing.assert_array_equal(np.sort(union), np.arange(total))
        self.assertEqual(np.intersect1d(train, val).size, 0)

    def test_no_shard_straddles_the_boundary(self):
        # Every validation index must belong to a shard whose whole span is on the
        # validation side; likewise for training. Checking the reported shard names
        # against the spans proves no shard is split.
        records = [7, 13, 5, 11, 9, 3]
        manifest = _manifest(records)
        spans = {s.name: s for s in shard_spans(manifest)}
        split = by_shard_split(manifest, val_fraction=0.3, seed=1)
        self._assert_partition(split, sum(records))

        val_set = set(split.val_idx.tolist())
        for name in split.val_shards:
            span = spans[name]
            self.assertTrue(set(range(span.start, span.stop)).issubset(val_set))
        train_set = set(split.train_idx.tolist())
        for name in split.train_shards:
            span = spans[name]
            self.assertTrue(set(range(span.start, span.stop)).issubset(train_set))
        # No shard is named on both sides.
        self.assertEqual(set(split.val_shards) & set(split.train_shards), set())

    def test_split_is_byte_identical_across_calls(self):
        manifest = _manifest([7, 13, 5, 11, 9, 3])
        first = by_shard_split(manifest, val_fraction=0.25, seed=42)
        second = by_shard_split(manifest, val_fraction=0.25, seed=42)
        np.testing.assert_array_equal(first.train_idx, second.train_idx)
        np.testing.assert_array_equal(first.val_idx, second.val_idx)
        self.assertEqual(first.val_shards, second.val_shards)

    def test_a_different_seed_can_change_the_split(self):
        manifest = _manifest([7, 13, 5, 11, 9, 3, 8, 4])
        a = by_shard_split(manifest, val_fraction=0.25, seed=1)
        b = by_shard_split(manifest, val_fraction=0.25, seed=2)
        # Not guaranteed different for every pair, but this corpus/seed pair is.
        self.assertNotEqual(a.val_shards, b.val_shards)

    def test_validation_reaches_the_requested_fraction(self):
        # Whole-shard granularity cannot hit the fraction exactly, but it must first
        # reach it: the held-out records are at least val_fraction of the corpus.
        records = [10] * 10
        total = sum(records)
        split = by_shard_split(_manifest(records), val_fraction=0.3, seed=7)
        self.assertGreaterEqual(split.val_idx.size, 0.3 * total)
        # And it does not over-reserve wildly: dropping the last held-out shard would
        # fall below the target, i.e. the cover is minimal.
        self.assertLess(split.val_idx.size - 10, 0.3 * total)


class DegenerateSplitTest(unittest.TestCase):
    def test_single_shard_cannot_be_split(self):
        with self.assertRaises(SplitError):
            by_shard_split(_manifest([100]), val_fraction=0.1, seed=0)

    def test_training_side_is_never_emptied(self):
        # A fraction that would swallow the whole corpus still leaves a training shard.
        manifest = _manifest([5, 5, 5, 5])
        split = by_shard_split(manifest, val_fraction=0.99, seed=0)
        self.assertGreater(split.train_idx.size, 0)
        self.assertGreater(split.val_idx.size, 0)
        self.assertEqual(len(split.train_shards), 1)

    def test_out_of_range_fraction_is_rejected(self):
        for bad in (0.0, 1.0, -0.1, 1.5):
            with self.assertRaises(SplitError):
                by_shard_split(_manifest([5, 5]), val_fraction=bad, seed=0)


class TrainerWiringTest(unittest.TestCase):
    """The by-shard split reaches training through ``train.py``'s CLI, and the corpus
    it validates against must match the manifest that defines the split."""

    def _corpus_and_manifest(self, tmp, shard_records: list[int]):
        import testsupport

        # One trivial record per position (a lone white pawn); only the split cares
        # about counts, not contents. concat writes one stream header then all bodies,
        # so a single encode_stream over the flat record list mirrors a joined corpus.
        total = sum(shard_records)
        records = [testsupport.encode_record({8 + i % 40: testsupport.WHITE_PAWN}) for i in range(total)]
        corpus = tmp / "corpus.bin"
        corpus.write_bytes(testsupport.encode_stream(records))
        (tmp / "corpus.manifest.json").write_text(json.dumps(_manifest(shard_records)))
        return corpus

    def test_cli_by_shard_split_runs_and_reports_the_holdout(self):
        import contextlib
        import io
        import tempfile
        from pathlib import Path

        import train

        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            corpus = self._corpus_and_manifest(tmp, [8, 4, 4])
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = train.main([
                    "--data", str(corpus), "--split", "by-shard",
                    "--val-fraction", "0.3", "--split-seed", "0",
                    "--epochs", "1", "--batch-size", "4", "--hidden", "16",
                ])
            self.assertEqual(code, 0)
            self.assertIn("by-shard split:", out.getvalue())

    def test_cli_rejects_a_corpus_that_disagrees_with_the_manifest(self):
        import tempfile
        from pathlib import Path

        import train

        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            corpus = self._corpus_and_manifest(tmp, [8, 4])  # corpus holds 12 records
            # Rewrite the manifest to claim a different total than the corpus holds.
            (tmp / "corpus.manifest.json").write_text(json.dumps(_manifest([8, 8])))
            with self.assertRaises(SystemExit):
                train.main([
                    "--data", str(corpus), "--split", "by-shard",
                    "--val-fraction", "0.3", "--epochs", "1", "--hidden", "16",
                ])


class LoadManifestTest(unittest.TestCase):
    def test_reads_from_a_directory_or_a_file(self):
        import tempfile
        from pathlib import Path

        manifest = _manifest([3, 4])
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "corpus.manifest.json"
            path.write_text(json.dumps(manifest))
            self.assertEqual(load_manifest(path), manifest)
            self.assertEqual(load_manifest(Path(tmp)), manifest)


if __name__ == "__main__":
    unittest.main()
