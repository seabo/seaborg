"""Tests for the version-2 bucketed multi-layer topology: the trainable model with
per-bucket int8 output stacks, the integer export, and their cross-language
correspondence.

They pin down what the version-2 path must get right: the config validation mirrors
the engine loader's version-2 rules; the model routes each sample through its
piece-count bucket; the exported integer network reproduces the quantization-aware
model's own evaluation to within the stack's rounding; the serialized bytes are the
exact format the engine loader reads (checked with the independent reader); and the
golden fixture spans multiple buckets with distinct per-layer scales, so it proves
the per-layer-scale path rather than a hard-coded ``qb``."""

from __future__ import annotations

import unittest

import numpy as np
import torch

import export
from export import (
    GOLDEN_POSITIONS,
    ExportError,
    QuantizedBucketedNetwork,
    features_from_fen,
    integer_eval_cp_bucketed,
    quantize_bucketed,
    select_bucket,
)
from model import NnueConfig, NnueModel

# The integer stack reproduces the quantization-aware model's centipawn output to
# within a few centipawns: unlike the single-layer version-1 path (bounded by the
# final 0.5cp divide), each hidden layer's requantize rounds half away from zero
# while the trainer's fake-quantize rounds half to even, so an exact-half unit can
# differ by one QA step and propagate. The contract documents this float-vs-integer
# gap; the integer paths still agree with each other bit for bit. The measured
# worst over these fixtures is well under a centipawn for CReLU and under two for
# SCReLU; this bound keeps headroom for other weight distributions while still
# catching a real arithmetic divergence.
_REPRODUCTION_TOLERANCE_CP = 4.0


def _bucketed_config(**overrides) -> NnueConfig:
    base = dict(
        hidden=16,
        activation="crelu",
        qb=256,
        num_buckets=8,
        output_stack=(16, 32),
        output_stack_scales=(64, 128, 256),
    )
    base.update(overrides)
    return NnueConfig(**base)


def _model_forward_cp(model, cfg, stm, nstm, piece_count):
    off = torch.tensor([0], dtype=torch.long)
    bucket = torch.tensor([select_bucket(piece_count, cfg.num_buckets)], dtype=torch.long)
    with torch.no_grad():
        fout = model(
            torch.from_numpy(np.asarray(stm)),
            off,
            torch.from_numpy(np.asarray(nstm)),
            off,
            bucket,
        ).item()
    return fout * cfg.scale


class ConfigValidationTest(unittest.TestCase):
    def test_valid_bucketed_config(self):
        _bucketed_config().validate()  # does not raise

    def test_v1_config_is_unchanged(self):
        # No output_stack -> version 1; num_buckets defaults to 1.
        cfg = NnueConfig(hidden=32)
        cfg.validate()
        self.assertFalse(cfg.is_bucketed)
        self.assertEqual(cfg.num_buckets, 1)

    def test_rejects_bad_bucket_count(self):
        with self.assertRaises(ValueError):
            _bucketed_config(num_buckets=0).validate()
        with self.assertRaises(ValueError):
            _bucketed_config(num_buckets=33).validate()

    def test_rejects_hidden_stack_dim_not_multiple_of_16(self):
        with self.assertRaises(ValueError):
            _bucketed_config(output_stack=(24, 32)).validate()

    def test_rejects_final_scale_not_qb(self):
        with self.assertRaises(ValueError):
            _bucketed_config(qb=64).validate()  # scales end in 256, qb is 64

    def test_rejects_scale_count_mismatch(self):
        with self.assertRaises(ValueError):
            _bucketed_config(output_stack_scales=(64, 256)).validate()

    def test_rejects_num_buckets_on_v1(self):
        with self.assertRaises(ValueError):
            NnueConfig(hidden=16, num_buckets=4).validate()


class SelectBucketTest(unittest.TestCase):
    def test_matches_the_binning_rule(self):
        self.assertEqual(select_bucket(2, 8), 0)
        self.assertEqual(select_bucket(5, 8), 1)
        self.assertEqual(select_bucket(32, 8), 7)
        self.assertEqual(select_bucket(0, 8), 0)  # never underflows
        self.assertEqual(select_bucket(20, 1), 0)  # single bucket
        self.assertEqual(select_bucket(32, 4), 3)


class BucketedModelTest(unittest.TestCase):
    def test_model_builds_the_stack(self):
        cfg = _bucketed_config()
        model = NnueModel(cfg)
        # One weight/bias per stack layer, each with a leading bucket dimension.
        self.assertEqual(len(model.stack_weights), len(cfg.stack_layer_dims))
        self.assertEqual(model.stack_weights[0].shape, (cfg.num_buckets, 16, 2 * cfg.hidden))
        self.assertEqual(model.stack_weights[-1].shape, (cfg.num_buckets, 1, 32))

    def test_forward_routes_by_bucket(self):
        # The same activated input under two different buckets reads different stacks
        # and so generally produces different outputs.
        torch.manual_seed(1)
        cfg = _bucketed_config()
        model = NnueModel(cfg)
        model.eval()
        stm, nstm = features_from_fen(GOLDEN_POSITIONS[0][1])
        off = torch.tensor([0], dtype=torch.long)
        outs = []
        for b in (0, 3, 7):
            bucket = torch.tensor([b], dtype=torch.long)
            with torch.no_grad():
                outs.append(
                    model(
                        torch.from_numpy(stm), off, torch.from_numpy(nstm), off, bucket
                    ).item()
                )
        self.assertGreater(len(set(outs)), 1, "buckets should route to different stacks")

    def test_forward_requires_bucket_for_bucketed(self):
        model = NnueModel(_bucketed_config())
        stm, nstm = features_from_fen(GOLDEN_POSITIONS[0][1])
        off = torch.tensor([0], dtype=torch.long)
        with self.assertRaises(ValueError):
            model(torch.from_numpy(stm), off, torch.from_numpy(nstm), off, None)


class BucketedExportTest(unittest.TestCase):
    def _trained_model(self, activation="crelu"):
        torch.manual_seed(7)
        cfg = _bucketed_config(activation=activation)
        model = NnueModel(cfg, quantization_aware=True)
        # Non-trivial weights so the correspondence check exercises a real value
        # range rather than a near-zero network; then clamp back into the grids.
        with torch.no_grad():
            model.feature_transformer.weight.normal_(0.0, 0.3)
            for w in model.stack_weights:
                w.normal_(0.0, 0.3)
            for b in model.stack_biases:
                b.normal_(0.0, 0.1)
        model.clamp_for_quantization()
        model.eval()
        return cfg, model

    def test_quantize_round_trips_through_bytes(self):
        _, model = self._trained_model()
        net = quantize_bucketed(model)
        data = net.to_bytes()
        back = QuantizedBucketedNetwork.from_bytes(data)
        self.assertEqual(net.layer_dims, back.layer_dims)
        self.assertEqual(net.layer_scales, back.layer_scales)
        self.assertTrue(np.array_equal(net.w_ft, back.w_ft))
        for b in range(net.num_buckets):
            for k in range(net.num_layers):
                self.assertTrue(np.array_equal(net.buckets[b][k][0], back.buckets[b][k][0]))
                self.assertTrue(np.array_equal(net.buckets[b][k][1], back.buckets[b][k][1]))
        self.assertEqual(data, back.to_bytes())

    def test_stack_weights_are_int8(self):
        _, model = self._trained_model()
        net = quantize_bucketed(model)
        for bucket in net.buckets:
            for w, _ in bucket:
                self.assertEqual(w.dtype, np.int8)
                self.assertTrue(w.min() >= -127 and w.max() <= 127)

    def _reproduction_error(self, activation):
        cfg, model = self._trained_model(activation)
        net = quantize_bucketed(model)
        worst = 0.0
        for _, fen in GOLDEN_POSITIONS:
            stm, nstm = features_from_fen(fen)
            pc = len(stm)
            model_cp = _model_forward_cp(model, cfg, stm, nstm, pc)
            int_cp = integer_eval_cp_bucketed(net, stm, nstm, pc)
            worst = max(worst, abs(model_cp - int_cp))
        return worst

    def test_integer_export_reproduces_the_model_crelu(self):
        worst = self._reproduction_error("crelu")
        self.assertLessEqual(worst, _REPRODUCTION_TOLERANCE_CP, f"max reproduction {worst:.2f}cp")

    def test_integer_export_reproduces_the_model_screlu(self):
        worst = self._reproduction_error("screlu")
        self.assertLessEqual(worst, _REPRODUCTION_TOLERANCE_CP, f"max reproduction {worst:.2f}cp")


class GoldenBucketedTest(unittest.TestCase):
    def test_golden_spans_buckets_with_distinct_scales(self):
        net = export._golden_bucketed_network()
        # Distinct per-layer scales, or the fixture would not prove the per-layer path.
        self.assertNotEqual(len(set(net.layer_scales)), 1)
        vectors = export.golden_bucketed_vectors(net)
        buckets = set()
        for _, fen, _ in vectors:
            stm, _ = features_from_fen(fen)
            buckets.add(select_bucket(len(stm), net.num_buckets))
        self.assertGreaterEqual(len(buckets), 2, "golden positions must span >= 2 buckets")

    def test_per_layer_scale_changes_the_quantized_weights(self):
        # Doubling a layer's scale doubles its rounded int8 weights: the scale is
        # honored per layer rather than a single qb applied everywhere.
        torch.manual_seed(3)
        cfg_a = _bucketed_config(output_stack_scales=(64, 128, 256))
        model = NnueModel(cfg_a, quantization_aware=True)
        with torch.no_grad():
            for w in model.stack_weights:
                w.normal_(0.0, 0.2)
        model.clamp_for_quantization()
        net_a = quantize_bucketed(model)
        # Re-quantize the identical float weights at a different first-layer scale.
        cfg_b = _bucketed_config(output_stack_scales=(32, 128, 256))
        model.config = cfg_b
        net_b = quantize_bucketed(model)
        # Layer 0's int weights differ because its scale differs; layer 2 (same
        # scale) is unchanged.
        self.assertFalse(np.array_equal(net_a.buckets[0][0][0], net_b.buckets[0][0][0]))
        self.assertTrue(np.array_equal(net_a.buckets[0][2][0], net_b.buckets[0][2][0]))


if __name__ == "__main__":
    unittest.main()
