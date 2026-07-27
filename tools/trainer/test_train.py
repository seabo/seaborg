"""Tests for the training target's schedulable blend weight ``lambda``.

The loss blends the engine's own search score with the self-play game outcome;
``lambda`` weights the outcome. It is configurable and scheduled over
reinforcement generations, and these tests pin the schedule's arithmetic and its
effect on the blended target down on a small hand-built fixture."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import numpy as np

import data
from model import NnueConfig
from testsupport import BLACK_KING, WHITE_KING, WHITE_PAWN, encode_record, encode_stream
from train import LambdaSchedule, resolve_lambda, targets, train


class LambdaScheduleTest(unittest.TestCase):
    def test_constant_ignores_the_generation(self):
        schedule = LambdaSchedule.constant(0.3)
        for generation in (0, 1, 5, 100):
            self.assertEqual(schedule.at(generation), 0.3)

    def test_ramp_interpolates_linearly_between_the_ends(self):
        # The contract's documented ramp: 0.1 -> 0.5 across the generations.
        schedule = LambdaSchedule.ramp(0.1, 0.5, generations=5)
        self.assertAlmostEqual(schedule.at(0), 0.1)  # first generation
        self.assertAlmostEqual(schedule.at(4), 0.5)  # last generation
        self.assertAlmostEqual(schedule.at(2), 0.3)  # midpoint

    def test_ramp_clamps_outside_its_range(self):
        schedule = LambdaSchedule.ramp(0.1, 0.5, generations=5)
        self.assertAlmostEqual(schedule.at(-3), 0.1)
        self.assertAlmostEqual(schedule.at(99), 0.5)

    def test_single_generation_ramp_is_the_start(self):
        # A degenerate range has no span to interpolate over.
        self.assertAlmostEqual(LambdaSchedule.ramp(0.1, 0.5, generations=1).at(0), 0.1)

    def test_ramp_requires_a_generation(self):
        with self.assertRaises(ValueError):
            LambdaSchedule.ramp(0.1, 0.5, generations=0)

    def test_resolve_lambda_accepts_a_bare_float_or_a_schedule(self):
        self.assertEqual(resolve_lambda(0.42, generation=7), 0.42)
        self.assertAlmostEqual(
            resolve_lambda(LambdaSchedule.ramp(0.1, 0.5, generations=5), generation=4), 0.5
        )


class TargetBlendTest(unittest.TestCase):
    """A small fixture of (search score, outcome) pairs, checking the loss's target
    combines the two exactly as the contract's blend prescribes and that the
    schedule's resolved lambda drives that blend."""

    def setUp(self):
        # Two positions: a searched-winning one that was won, and a searched-losing
        # one that was drawn -- so score and outcome disagree and the blend matters.
        self.score = np.array([300, -300], dtype=np.int64)
        self.wdl = np.array([2, 1], dtype=np.int64)  # win, draw
        self.scale = 400.0

    def _expected(self, lam: float) -> np.ndarray:
        r = self.wdl / 2.0
        score_target = 1.0 / (1.0 + np.exp(-self.score / self.scale))
        return lam * r + (1.0 - lam) * score_target

    def test_resolved_lambda_blends_search_and_outcome(self):
        schedule = LambdaSchedule.ramp(0.1, 0.5, generations=5)
        for generation in (0, 2, 4):
            lam = resolve_lambda(schedule, generation)
            got = targets(self.score, self.wdl, self.scale, lam)
            np.testing.assert_allclose(got, self._expected(lam))

    def test_endpoints_trust_search_or_outcome(self):
        # lambda = 0 is the pure search target; lambda = 1 is the pure outcome.
        search_only = targets(self.score, self.wdl, self.scale, 0.0)
        outcome_only = targets(self.score, self.wdl, self.scale, 1.0)
        np.testing.assert_allclose(
            search_only, 1.0 / (1.0 + np.exp(-self.score / self.scale))
        )
        np.testing.assert_allclose(outcome_only, self.wdl / 2.0)

    def test_schedule_actually_changes_the_target(self):
        # Different generations of a ramp must produce genuinely different targets,
        # or the schedule would be doing nothing.
        schedule = LambdaSchedule.ramp(0.1, 0.5, generations=5)
        early = targets(self.score, self.wdl, self.scale, resolve_lambda(schedule, 0))
        late = targets(self.score, self.wdl, self.scale, resolve_lambda(schedule, 4))
        self.assertFalse(np.allclose(early, late))


class ParallelTrainingEquivalenceTest(unittest.TestCase):
    """Training with decode workers must reach exactly the same place as serial
    training: identical batches in identical order, plus a fixed seed, means the
    optimisation trajectory does not depend on the worker count. This is the
    property that lets the sweep switch on parallel decoding without disturbing the
    fixed-config comparison between candidates."""

    def _fixture(self, n: int) -> Path:
        records = [
            encode_record(
                {4: WHITE_KING, 60: BLACK_KING, 8 + (i % 48): WHITE_PAWN},
                black_to_move=bool(i % 2),
                score=(i * 11) % 600 - 300,
                wdl=i % 3,
            )
            for i in range(n)
        ]
        handle = tempfile.NamedTemporaryFile(suffix=".bin", delete=False)
        handle.write(encode_stream(records))
        handle.close()
        path = Path(handle.name)
        self.addCleanup(path.unlink)
        return path

    def _train_history(self, packed, num_workers):
        # A tiny net and a fixed seed so the run is fast and fully determined by the
        # batch stream; only num_workers varies between the two calls.
        _, history = train(
            packed,
            NnueConfig(hidden=16),
            epochs=3,
            batch_size=8,
            lr=1e-2,
            lam=0.3,
            val_fraction=0.2,
            seed=1234,
            device="cpu",
            num_workers=num_workers,
            log=lambda *_: None,
        )
        return [(round(r.train_loss, 10), round(r.val_loss, 10)) for r in history]

    def test_worker_count_does_not_change_the_trajectory(self):
        packed = data.PackedData(self._fixture(120))
        serial = self._train_history(packed, num_workers=1)
        parallel = self._train_history(packed, num_workers=3)
        self.assertEqual(serial, parallel)


if __name__ == "__main__":
    unittest.main()
