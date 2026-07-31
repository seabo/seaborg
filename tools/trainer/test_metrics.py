"""Tests for the value-fidelity metrics: the centipawn/win-prob/winner/calibration
math on a hand-computed fixture, mate-band exclusion, the deadband, streaming
additivity, and numerically stable sigmoid. NumPy only -- no PyTorch."""

from __future__ import annotations

import math
import unittest

import numpy as np

from metrics import ValueFidelityAccumulator, _sigmoid

SCALE = 400.0

# (fout, teacher score cp, wdl): a quiet agreement, a quiet disagreement in
# magnitude, a mate-band score (excluded from cp error), and a near-draw inside
# the winner deadband.
_FOUT = np.array([0.25, -0.5, 1.0, 0.0])
_SCORE = np.array([100, -100, 30000, 5])
_WDL = np.array([2, 0, 2, 1])


def _sig(x: float) -> float:
    return 1.0 / (1.0 + math.exp(-x))


class ValueFidelityMathTest(unittest.TestCase):
    def _acc(self):
        acc = ValueFidelityAccumulator(SCALE)
        acc.update(_FOUT, _SCORE, _WDL)
        return acc.result()

    def test_cp_error_excludes_the_mate_score(self):
        r = self._acc()
        self.assertEqual(r.n, 4)
        self.assertEqual(r.mate_excluded, 1)  # the 30000 score
        self.assertEqual(r.cp_n, 3)
        # abs cp errors over non-mate: |100-100|, |-200-(-100)|, |0-5| = 0,100,5
        self.assertAlmostEqual(r.cp_mae, 105.0 / 3.0, places=4)
        self.assertAlmostEqual(r.cp_rmse, math.sqrt((0 + 100**2 + 5**2) / 3.0), places=3)

    def test_winner_agreement_ignores_the_deadband(self):
        r = self._acc()
        # decisive = |teacher| >= 20 -> positions 0,1,2 (the |5| one is excluded)
        self.assertEqual(r.winner_coverage, 3)
        self.assertAlmostEqual(r.winner_agreement, 1.0, places=6)

    def test_winner_agreement_below_one_on_a_sign_flip(self):
        acc = ValueFidelityAccumulator(SCALE)
        # add a decisive disagreement: predicts white better, teacher says black
        acc.update(
            np.array([0.25, -0.5, 0.5]),
            np.array([100, -100, -100]),
            np.array([2, 0, 0]),
        )
        r = acc.result()
        self.assertEqual(r.winner_coverage, 3)
        self.assertAlmostEqual(r.winner_agreement, 2.0 / 3.0, places=6)

    def test_win_prob_error_mean(self):
        r = self._acc()
        errs = [
            abs(_sig(0.25) - _sig(0.25)),
            abs(_sig(-0.5) - _sig(-0.25)),
            abs(_sig(1.0) - _sig(75.0)),
            abs(_sig(0.0) - _sig(5.0 / SCALE)),
        ]
        self.assertAlmostEqual(r.wp_mae, sum(errs) / 4.0, places=5)

    def test_win_prob_percentiles_bounded_and_monotone(self):
        r = self._acc()
        self.assertTrue(0.0 <= r.wp_err_p50 <= r.wp_err_p90 <= r.wp_err_p99 <= 1.0)
        # the tail must capture the large 0.269 error from the mate row
        self.assertGreater(r.wp_err_p99, 0.25)
        self.assertLess(r.wp_err_p50, 0.05)

    def test_calibration_bins_by_predicted_win_prob(self):
        r = self._acc()
        # pred win-probs 0.562, 0.378, 0.731, 0.500 -> bins 5, 3, 7, 5
        b5 = r.calibration[5]
        self.assertEqual(b5.count, 2)  # the 0.562 and 0.500 rows
        self.assertAlmostEqual(b5.mean_outcome, (1.0 + 0.5) / 2.0, places=6)  # wdl 2 and 1
        self.assertEqual(r.calibration[3].count, 1)
        self.assertEqual(r.calibration[7].count, 1)

    def test_streaming_is_additive(self):
        whole = ValueFidelityAccumulator(SCALE)
        whole.update(_FOUT, _SCORE, _WDL)
        a = whole.result()

        parts = ValueFidelityAccumulator(SCALE)
        parts.update(_FOUT[:2], _SCORE[:2], _WDL[:2])
        parts.update(_FOUT[2:], _SCORE[2:], _WDL[2:])
        b = parts.result()

        self.assertEqual((a.n, a.cp_n, a.mate_excluded), (b.n, b.cp_n, b.mate_excluded))
        self.assertAlmostEqual(a.cp_mae, b.cp_mae, places=9)
        self.assertAlmostEqual(a.wp_mae, b.wp_mae, places=9)
        self.assertAlmostEqual(a.winner_agreement, b.winner_agreement, places=9)
        self.assertAlmostEqual(a.wp_err_p90, b.wp_err_p90, places=9)

    def test_sigmoid_is_stable_at_extremes(self):
        out = _sigmoid(np.array([-1000.0, 0.0, 1000.0]))
        self.assertTrue(np.all(np.isfinite(out)))
        self.assertAlmostEqual(out[0], 0.0, places=12)
        self.assertAlmostEqual(out[1], 0.5, places=12)
        self.assertAlmostEqual(out[2], 1.0, places=12)


if __name__ == "__main__":
    unittest.main()
