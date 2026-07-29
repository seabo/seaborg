"""Interpretable value-fidelity metrics for a trained evaluation network.

Validation loss (blended win-probability MSE) answers "how well does the net fit
the training target" but not, legibly, "how good is this eval". These metrics
compare the network's static eval directly to the **teacher search score** (the
label) on the held-out validation set, in units you can reason about:

- **cp error** — mean and RMS disagreement with the teacher, in centipawns.
- **winner agreement** — how often the net agrees on *who is better*.
- **win-probability error percentiles** — the typical and tail size of the error
  in outcome-probability space (bounded, so robust to mate scores).
- **calibration** — bucket positions by the net's predicted win probability and
  compare to the realised game outcome, so we can see whether the eval is honest.

Units are fixed by the trainer (see ``model.py``): the model output ``fout``
equals ``eval_cp / SCALE``, and the prediction is ``sigmoid(fout)``. The teacher
score is a raw centipawn value whose win probability is ``sigmoid(score / SCALE)``.
So predicted centipawns are ``fout * SCALE`` and the teacher's are ``score``.

The reference for fidelity is the *pure* teacher search score, not the
``lambda``-blended training target: we are asking how faithfully the eval
reproduces the search it is meant to distil, not how well it fits the blend.

The accumulator is streaming (running sums plus a fixed histogram for the
percentiles), so it scales to a corpus of any size in bounded memory.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

# Scores at or beyond this magnitude are the mate band (raw i16, mate-distance
# preserved), not a positional evaluation. They are excluded from the centipawn
# error — where they would otherwise dominate the mean — and counted separately.
# Win-probability metrics keep them: they saturate harmlessly near 0/1.
MATE_CP_THRESHOLD = 20_000

# Positions within this centipawn band of equality are near-draws whose sign is
# noise; the winner-agreement rate is measured on the decisive positions outside
# it, so it reflects real judgement rather than coin-flips on dead-equal boards.
WINNER_DEADBAND_CP = 20


def _sigmoid(x: np.ndarray) -> np.ndarray:
    """Numerically stable logistic, elementwise."""
    return np.where(x >= 0, 1.0 / (1.0 + np.exp(-x)), np.exp(x) / (1.0 + np.exp(x)))


@dataclass
class CalibrationBin:
    lo: float
    hi: float
    count: int
    mean_pred_wp: float
    mean_teacher_wp: float
    mean_outcome: float


@dataclass
class ValueFidelity:
    """The computed metrics. ``cp_*`` cover only non-mate positions (``cp_n`` of
    ``n`` total, ``mate_excluded`` dropped); everything else covers all ``n``."""

    n: int
    cp_n: int
    mate_excluded: int
    cp_mae: float
    cp_rmse: float
    winner_agreement: float
    winner_coverage: int
    winner_deadband_cp: int
    wp_mae: float
    wp_err_p50: float
    wp_err_p90: float
    wp_err_p99: float
    calibration: list[CalibrationBin] = field(default_factory=list)


class ValueFidelityAccumulator:
    """Streams ``(fout, score, wdl)`` batches into the value-fidelity metrics."""

    def __init__(
        self,
        scale: float,
        *,
        mate_cp: int = MATE_CP_THRESHOLD,
        winner_deadband_cp: int = WINNER_DEADBAND_CP,
        calib_bins: int = 10,
        err_bins: int = 2000,
    ) -> None:
        self.scale = float(scale)
        self.mate_cp = int(mate_cp)
        self.winner_deadband_cp = int(winner_deadband_cp)
        self._n = 0
        # Centipawn error over non-mate positions.
        self._cp_n = 0
        self._mate = 0
        self._cp_abs = 0.0
        self._cp_sq = 0.0
        # Winner agreement over decisive positions.
        self._win_agree = 0
        self._win_cov = 0
        # Win-probability error: running mean plus a histogram for percentiles.
        self._wp_abs = 0.0
        self._err_edges = np.linspace(0.0, 1.0, err_bins + 1)
        self._err_hist = np.zeros(err_bins, dtype=np.int64)
        # Calibration, bucketed by predicted win probability.
        self._calib_bins = calib_bins
        self._c_count = np.zeros(calib_bins, dtype=np.int64)
        self._c_pred = np.zeros(calib_bins, dtype=np.float64)
        self._c_teacher = np.zeros(calib_bins, dtype=np.float64)
        self._c_outcome = np.zeros(calib_bins, dtype=np.float64)

    def update(self, fout: np.ndarray, score: np.ndarray, wdl: np.ndarray) -> None:
        fout = np.asarray(fout, dtype=np.float64).reshape(-1)
        teacher_cp = np.asarray(score, dtype=np.float64).reshape(-1)
        pred_cp = fout * self.scale
        pred_wp = _sigmoid(fout)
        teacher_wp = _sigmoid(teacher_cp / self.scale)
        outcome = np.asarray(wdl, dtype=np.float64).reshape(-1) / 2.0
        self._n += fout.size

        nonmate = np.abs(teacher_cp) < self.mate_cp
        d = (pred_cp - teacher_cp)[nonmate]
        self._cp_abs += np.abs(d).sum()
        self._cp_sq += np.square(d).sum()
        self._cp_n += int(nonmate.sum())
        self._mate += int((~nonmate).sum())

        decisive = np.abs(teacher_cp) >= self.winner_deadband_cp
        self._win_agree += int((np.sign(fout) == np.sign(teacher_cp))[decisive].sum())
        self._win_cov += int(decisive.sum())

        wp_err = np.abs(pred_wp - teacher_wp)
        self._wp_abs += wp_err.sum()
        self._err_hist += np.histogram(wp_err, bins=self._err_edges)[0]

        idx = np.clip((pred_wp * self._calib_bins).astype(np.int64), 0, self._calib_bins - 1)
        np.add.at(self._c_count, idx, 1)
        np.add.at(self._c_pred, idx, pred_wp)
        np.add.at(self._c_teacher, idx, teacher_wp)
        np.add.at(self._c_outcome, idx, outcome)

    def _percentile(self, q: float) -> float:
        """Approximate ``q``-quantile of the win-prob error from the histogram.
        Resolution is one histogram bin (default 1/2000)."""
        total = self._err_hist.sum()
        if total == 0:
            return float("nan")
        target = q * total
        cum = np.cumsum(self._err_hist)
        i = int(np.searchsorted(cum, target, side="left"))
        i = min(i, self._err_hist.size - 1)
        # Upper edge of the crossing bin: a conservative, monotone estimate.
        return float(self._err_edges[i + 1])

    def result(self) -> ValueFidelity:
        cp_n = max(self._cp_n, 1)
        calib = []
        for b in range(self._calib_bins):
            c = int(self._c_count[b])
            calib.append(
                CalibrationBin(
                    lo=b / self._calib_bins,
                    hi=(b + 1) / self._calib_bins,
                    count=c,
                    mean_pred_wp=float(self._c_pred[b] / c) if c else float("nan"),
                    mean_teacher_wp=float(self._c_teacher[b] / c) if c else float("nan"),
                    mean_outcome=float(self._c_outcome[b] / c) if c else float("nan"),
                )
            )
        return ValueFidelity(
            n=self._n,
            cp_n=self._cp_n,
            mate_excluded=self._mate,
            cp_mae=self._cp_abs / cp_n,
            cp_rmse=float(np.sqrt(self._cp_sq / cp_n)),
            winner_agreement=(self._win_agree / self._win_cov) if self._win_cov else float("nan"),
            winner_coverage=self._win_cov,
            winner_deadband_cp=self.winner_deadband_cp,
            wp_mae=self._wp_abs / max(self._n, 1),
            wp_err_p50=self._percentile(0.50),
            wp_err_p90=self._percentile(0.90),
            wp_err_p99=self._percentile(0.99),
            calibration=calib,
        )


def format_value_fidelity(vf: ValueFidelity) -> str:
    """A compact multi-line summary, for logging beside the val loss."""
    lines = [
        f"value fidelity vs teacher search score ({vf.n:,} val positions):",
        f"  cp error:         MAE {vf.cp_mae:7.1f}  RMSE {vf.cp_rmse:7.1f} cp "
        f"(over {vf.cp_n:,} non-mate; {vf.mate_excluded:,} mate excluded)",
        f"  winner agreement: {vf.winner_agreement * 100:5.1f}% "
        f"(on {vf.winner_coverage:,} decisive positions, deadband {vf.winner_deadband_cp} cp)",
        f"  win-prob error:   MAE {vf.wp_mae:.4f}  p50 {vf.wp_err_p50:.4f}  "
        f"p90 {vf.wp_err_p90:.4f}  p99 {vf.wp_err_p99:.4f}",
        "  calibration (pred win-prob -> mean teacher / mean outcome):",
    ]
    for b in vf.calibration:
        if b.count == 0:
            continue
        lines.append(
            f"    [{b.lo:.1f},{b.hi:.1f})  pred {b.mean_pred_wp:.3f}  "
            f"teacher {b.mean_teacher_wp:.3f}  outcome {b.mean_outcome:.3f}  (n={b.count:,})"
        )
    return "\n".join(lines)


def value_fidelity_dict(vf: ValueFidelity) -> dict:
    """A plain-dict form for storing in a checkpoint / report."""
    return {
        "n": vf.n,
        "cp_n": vf.cp_n,
        "mate_excluded": vf.mate_excluded,
        "cp_mae": vf.cp_mae,
        "cp_rmse": vf.cp_rmse,
        "winner_agreement": vf.winner_agreement,
        "winner_coverage": vf.winner_coverage,
        "winner_deadband_cp": vf.winner_deadband_cp,
        "wp_mae": vf.wp_mae,
        "wp_err_p50": vf.wp_err_p50,
        "wp_err_p90": vf.wp_err_p90,
        "wp_err_p99": vf.wp_err_p99,
        "calibration": [
            {
                "lo": b.lo,
                "hi": b.hi,
                "count": b.count,
                "mean_pred_wp": b.mean_pred_wp,
                "mean_teacher_wp": b.mean_teacher_wp,
                "mean_outcome": b.mean_outcome,
            }
            for b in vf.calibration
        ],
    }
