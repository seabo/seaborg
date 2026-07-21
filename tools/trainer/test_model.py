"""Tests for the NNUE model and training pieces: configuration validation,
parameterization, the mirror-evaluation invariant, and that a short training run
reduces the loss."""

from __future__ import annotations

import unittest

import numpy as np
import torch

import data
import train
from model import NnueConfig, NnueModel
from testsupport import (
    BLACK_KING,
    WHITE_KING,
    WHITE_PAWN,
    encode_record,
    mirror,
)


def _batch_tensors(batch):
    return (
        torch.from_numpy(batch.stm_indices),
        torch.from_numpy(batch.offsets),
        torch.from_numpy(batch.nstm_indices),
        torch.from_numpy(batch.offsets),
    )


class ConfigValidationTest(unittest.TestCase):
    def test_hidden_width_must_be_a_positive_multiple_of_16(self):
        for bad in (0, 15, 100, -16):
            with self.assertRaises(ValueError):
                NnueConfig(hidden=bad).validate()
        NnueConfig(hidden=16).validate()  # accepted

    def test_unknown_activation_is_rejected(self):
        with self.assertRaises(ValueError):
            NnueConfig(activation="relu6").validate()

    def test_non_positive_scales_are_rejected(self):
        for cfg in (NnueConfig(qa=0), NnueConfig(qb=-1), NnueConfig(scale=0)):
            with self.assertRaises(ValueError):
                cfg.validate()


class ParameterizationTest(unittest.TestCase):
    def test_hidden_width_sizes_both_layers(self):
        model = NnueModel(NnueConfig(hidden=48))
        self.assertEqual(model.feature_transformer.weight.shape, (768, 48))
        self.assertEqual(model.ft_bias.shape, (48,))
        # The output layer reads the concatenation of both perspectives (2H).
        self.assertEqual(model.output.in_features, 96)

    def test_activation_id_matches_the_contract(self):
        self.assertEqual(NnueConfig(activation="crelu").activation_id, 0)
        self.assertEqual(NnueConfig(activation="screlu").activation_id, 1)

    def test_crelu_and_screlu_differ(self):
        pieces = {sq: (sq % 12) + 1 for sq in range(0, 64, 2)}
        batch = data.decode(np.stack([encode_record(pieces)]))
        args = _batch_tensors(batch)
        torch.manual_seed(0)
        crelu = NnueModel(NnueConfig(hidden=16, activation="crelu"))
        torch.manual_seed(0)
        screlu = NnueModel(NnueConfig(hidden=16, activation="screlu"))
        # Same weights, different activation -> different output.
        self.assertFalse(torch.allclose(crelu(*args), screlu(*args)))


class ForwardTest(unittest.TestCase):
    def test_output_is_one_scalar_per_sample(self):
        records = [
            encode_record({4: WHITE_KING, 60: BLACK_KING}),
            encode_record({0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING}),
        ]
        batch = data.decode(np.stack(records))
        out = NnueModel(NnueConfig(hidden=16))(*_batch_tensors(batch))
        self.assertEqual(out.shape, (2,))

    def test_mirror_positions_evaluate_equally(self):
        # From the side to move, a position and its colour/board mirror are the
        # same input, so any model must score them identically. This holds
        # without training -- it is a property of the architecture.
        pieces = {0: WHITE_PAWN, 4: WHITE_KING, 60: BLACK_KING, 33: BLACK_KING}
        original = data.decode(np.stack([encode_record(pieces, black_to_move=False)]))
        flipped = data.decode(np.stack([encode_record(mirror(pieces), black_to_move=True)]))
        model = NnueModel(NnueConfig(hidden=32))
        model.eval()
        with torch.no_grad():
            a = model(*_batch_tensors(original))
            b = model(*_batch_tensors(flipped))
        self.assertTrue(torch.allclose(a, b, atol=1e-6))


class TargetTest(unittest.TestCase):
    def test_target_blends_outcome_and_search(self):
        score = np.array([0], dtype=np.int64)
        # lambda=1 trusts the outcome entirely: a win -> 1.0, a loss -> 0.0.
        self.assertAlmostEqual(train.targets(score, np.array([2]), 400, 1.0)[0], 1.0)
        self.assertAlmostEqual(train.targets(score, np.array([0]), 400, 1.0)[0], 0.0)
        # lambda=0 trusts the search score: sigmoid(0) = 0.5 regardless of wdl.
        self.assertAlmostEqual(train.targets(score, np.array([2]), 400, 0.0)[0], 0.5)

    def test_scale_maps_centipawns_to_win_probability(self):
        # A +SCALE centipawn score is sigmoid(1) in win-probability space.
        y = train.targets(np.array([400]), np.array([1]), 400, 0.0)[0]
        self.assertAlmostEqual(y, 1.0 / (1.0 + np.exp(-1.0)))


class TrainingTest(unittest.TestCase):
    def test_a_short_run_reduces_loss_and_converges(self):
        # Build a synthetic dataset with a signal the network can actually fit:
        # the outcome is a function of the position (an advanced white pawn wins,
        # a back-rank one loses), and the score agrees with it. The pawn square
        # varies across samples, so the model must read its features rather than
        # predict a constant.
        rng = np.random.default_rng(0)
        records = []
        for _ in range(2000):
            pawn_sq = 8 + int(rng.integers(0, 48))  # ranks 2..7, never on a king
            winning = pawn_sq >= 32  # advanced pawn -> winning for White (to move)
            pieces = {4: WHITE_KING, 60: BLACK_KING, pawn_sq: WHITE_PAWN}
            score = 600 if winning else -600
            wdl = 2 if winning else 0
            records.append(encode_record(pieces, score=score, wdl=wdl))

        class _InMemory(data.PackedData):
            def __init__(self, records):
                self.records = np.stack(records)

        dataset = _InMemory(records)
        model, history = train.train(
            dataset,
            NnueConfig(hidden=32),
            epochs=15,
            batch_size=256,
            lr=1e-2,
            lam=0.5,
            val_fraction=0.2,
            seed=0,
        )
        self.assertLess(history[-1].train_loss, history[0].train_loss)
        # It should genuinely fit the signal, not merely dip.
        self.assertLess(history[-1].train_loss, 0.05)
        self.assertLess(history[-1].val_loss, 0.05)


if __name__ == "__main__":
    unittest.main()
