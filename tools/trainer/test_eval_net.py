"""Tests for the standalone value-fidelity evaluator (eval_net.py): that
``load_sbnn_model`` reconstructs the exact network an SBNN exported from, and that
the standalone path runs end to end over a corpus (load -> by-shard split ->
value-fidelity metrics)."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import numpy as np

import eval_net
from data import BatchLoader, PackedData
from export import quantize
from model import NnueConfig, NnueModel
from testsupport import BLACK_KING, WHITE_KING, WHITE_PAWN, encode_record, encode_stream
from train import value_fidelity_eval


def _small_v1_model(hidden: int = 16, seed: int = 0) -> NnueModel:
    """A fresh v1 (single-output) model, clamped so it quantizes without i16
    overflow — enough to exercise export/reconstruction."""
    import torch

    torch.manual_seed(seed)
    model = NnueModel(NnueConfig(hidden=hidden), quantization_aware=True)
    model.clamp_for_quantization()
    return model


def _corpus_bytes(n: int) -> bytes:
    """A packed stream of ``n`` distinct legal K+K(+P) positions."""
    records = [
        encode_record(
            {4: WHITE_KING, 60: BLACK_KING, 8 + (i % 48): WHITE_PAWN},
            black_to_move=bool(i % 2),
            score=(i * 13) % 500 - 250,
            wdl=i % 3,
        )
        for i in range(n)
    ]
    return encode_stream(records)


class EvalNetTest(unittest.TestCase):
    def _write(self, blob: bytes, suffix: str) -> Path:
        handle = tempfile.NamedTemporaryFile(suffix=suffix, delete=False)
        handle.write(blob)
        handle.close()
        path = Path(handle.name)
        self.addCleanup(path.unlink)
        return path

    def test_load_sbnn_model_roundtrips_exactly(self):
        # Dequantizing an SBNN back into a model is the exact inverse of the
        # quantized export: re-quantizing the reconstructed model must reproduce
        # the original bytes, byte for byte. A broken reconstruction (wrong
        # reshape, wrong bias rescale) would corrupt the weights and fail here.
        original = quantize(_small_v1_model())
        sbnn = original.to_bytes()
        path = self._write(sbnn, ".sbnn")

        model, scale = eval_net.load_sbnn_model(path)
        self.assertEqual(scale, original.scale)
        self.assertEqual(quantize(model).to_bytes(), sbnn)

    def test_value_fidelity_eval_over_a_corpus(self):
        packed = PackedData(self._write(_corpus_bytes(40), ".bin"))
        model, scale = eval_net.load_sbnn_model(self._write(quantize(_small_v1_model()).to_bytes(), ".sbnn"))
        indices = np.arange(len(packed))
        with BatchLoader(packed, 1) as loader:
            vf = value_fidelity_eval(model, loader, indices, batch_size=8, device="cpu", scale=scale)
        self.assertEqual(vf.n, 40)
        self.assertEqual(sum(b.count for b in vf.calibration), 40)
        self.assertTrue(np.isfinite(vf.cp_mae) and np.isfinite(vf.wp_mae))
        self.assertTrue(0.0 <= vf.winner_agreement <= 1.0)

    def test_standalone_main_runs_end_to_end(self):
        n = 40
        corpus = self._write(_corpus_bytes(n), ".bin")
        # A by-shard manifest tiling the corpus (four shards of ten), so the
        # standalone path exercises load_manifest + by_shard_split + the loader.
        manifest = {
            "shards": [
                {"file": f"shard_{i:03d}.bin", "opening_seed": 100 + i, "records": 10}
                for i in range(4)
            ],
            "total_records": n,
        }
        manifest_path = self._write(json.dumps(manifest).encode(), ".manifest.json")
        sbnn = self._write(quantize(_small_v1_model()).to_bytes(), ".sbnn")

        rc = eval_net.main(
            [
                "--net", str(sbnn),
                "--corpus", str(corpus),
                "--manifest", str(manifest_path),
                "--val-fraction", "0.5",
                "--num-workers", "1",
                "--device", "cpu",
            ]
        )
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
