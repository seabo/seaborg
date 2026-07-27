"""Tests for the architecture-sweep screen: candidate enumeration, the loss/NPS
domination and Pareto-frontier logic, finalist selection, and the SPRT hand-off.

These pin the parts a reviewer can verify without a GPU or the engine — the leak
in the split (test_split.py) and the frontier logic here are exactly what the
downstream campaign trusts when it spends days of training and thousands of games
on the finalists this screen names."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from sweep import (
    Architecture,
    Candidate,
    Screen,
    SprtConfig,
    SweepConfig,
    SweepError,
    SweepGrid,
    TrainingConfig,
    dominates,
    enumerate_candidates,
    pareto_frontier,
    read_param_hash,
    select_finalists,
    sprt_command,
    run_sweep,
)


def _screen(name: str, loss: float, nps: float) -> Screen:
    """A screened point whose ``candidate.factor`` carries the test's label; only the
    (loss, NPS) pair and that label matter to the frontier logic. Use :func:`_labels`
    to read the labels back out."""
    return Screen(
        candidate=Candidate(factor=name, arch=Architecture(hidden=256)),
        val_loss=loss,
        nps=nps,
        param_hash="0x" + "0" * 16,
        net_path=f"/nets/{name}.sbnn",
    )


def _labels(screens: list[Screen]) -> list[str]:
    """The test labels of screened points, in order (see :func:`_screen`)."""
    return [s.candidate.factor for s in screens]


class ArchitectureValidationTest(unittest.TestCase):
    def test_width_must_be_a_positive_multiple_of_16(self):
        for bad in (0, -16, 100):
            with self.assertRaises(SweepError):
                Architecture(hidden=bad).validate(qb=64)

    def test_buckets_need_a_stack(self):
        with self.assertRaises(SweepError):
            Architecture(hidden=256, num_buckets=4).validate(qb=64)

    def test_final_stack_scale_must_equal_qb(self):
        with self.assertRaises(SweepError):
            Architecture(
                hidden=256, num_buckets=4, output_stack=(16,), output_stack_scales=(64, 32)
            ).validate(qb=64)

    def test_a_valid_bucketed_shape_passes(self):
        Architecture(
            hidden=256, num_buckets=8, output_stack=(16, 32), output_stack_scales=(128, 128, 64)
        ).validate(qb=64)


class EnumerationTest(unittest.TestCase):
    def setUp(self):
        self.grid = SweepGrid()
        self.candidates = enumerate_candidates(self.grid)

    def test_every_candidate_is_unique_and_valid(self):
        keys = [c.arch.key() for c in self.candidates]
        self.assertEqual(len(keys), len(set(keys)))
        for c in self.candidates:
            c.arch.validate(self.grid.qb)

    def test_baseline_appears_exactly_once(self):
        baseline = [c for c in self.candidates if c.arch.key() == self.grid.baseline.key()]
        self.assertEqual(len(baseline), 1)
        # The width/activation factors both regenerate the baseline shape; dedup keeps
        # it under "baseline", not a second time.
        self.assertEqual(baseline[0].factor, "baseline")

    def test_each_factor_varies_exactly_one_dimension(self):
        base = self.grid.baseline
        reference = Architecture(
            hidden=base.hidden, activation=base.activation,
            output_stack=self.grid.reference_stack, num_buckets=self.grid.reference_buckets,
        )
        for c in self.candidates:
            arch = c.arch
            if c.factor == "width":
                # Only the width differs from the version-1 baseline.
                self.assertEqual(arch.activation, base.activation)
                self.assertFalse(arch.is_bucketed)
            elif c.factor == "activation":
                self.assertEqual(arch.hidden, base.hidden)
                self.assertFalse(arch.is_bucketed)
            elif c.factor == "buckets":
                # Only the bucket count differs from the version-2 reference.
                self.assertEqual(arch.hidden, reference.hidden)
                self.assertEqual(arch.output_stack, reference.output_stack)
                self.assertIsNone(arch.output_stack_scales)
            elif c.factor == "stack-depth":
                self.assertEqual(arch.num_buckets, reference.num_buckets)
                self.assertIsNone(arch.output_stack_scales)
            elif c.factor == "tail-quant":
                self.assertEqual(arch.output_stack, reference.output_stack)
                self.assertEqual(arch.num_buckets, reference.num_buckets)
                self.assertIsNotNone(arch.output_stack_scales)
                self.assertEqual(arch.output_stack_scales[-1], self.grid.qb)

    def test_names_are_unique(self):
        names = [c.name for c in self.candidates]
        self.assertEqual(len(names), len(set(names)))


class DominationTest(unittest.TestCase):
    def test_strictly_better_on_both_axes_dominates(self):
        self.assertTrue(dominates(_screen("a", 0.1, 200), _screen("b", 0.2, 100)))

    def test_better_on_one_and_equal_on_the_other_dominates(self):
        self.assertTrue(dominates(_screen("a", 0.1, 100), _screen("b", 0.2, 100)))
        self.assertTrue(dominates(_screen("a", 0.1, 200), _screen("b", 0.1, 100)))

    def test_identical_points_do_not_dominate_each_other(self):
        a, b = _screen("a", 0.1, 100), _screen("b", 0.1, 100)
        self.assertFalse(dominates(a, b))
        self.assertFalse(dominates(b, a))

    def test_a_trade_off_is_not_domination(self):
        # Lower loss but also lower NPS: neither dominates.
        a, b = _screen("a", 0.1, 100), _screen("b", 0.2, 200)
        self.assertFalse(dominates(a, b))
        self.assertFalse(dominates(b, a))


class FrontierTest(unittest.TestCase):
    def test_single_candidate_is_its_own_frontier(self):
        one = [_screen("a", 0.1, 100)]
        self.assertEqual(_labels(pareto_frontier(one)), ["a"])

    def test_dominated_points_are_dropped(self):
        screens = [
            _screen("fast", 0.30, 300),   # frontier: fastest
            _screen("mid", 0.20, 200),    # frontier: middle
            _screen("sharp", 0.10, 100),  # frontier: sharpest
            _screen("dominated", 0.25, 150),  # beaten by mid on both axes
        ]
        frontier = set(_labels(pareto_frontier(screens)))
        self.assertEqual(frontier, {"fast", "mid", "sharp"})

    def test_all_but_one_dominated_leaves_the_best(self):
        screens = [
            _screen("best", 0.10, 300),
            _screen("a", 0.20, 200),
            _screen("b", 0.30, 100),
        ]
        # "best" dominates both others (lower loss and higher nps).
        self.assertEqual(_labels(pareto_frontier(screens)), ["best"])

    def test_tied_frontier_points_all_survive(self):
        screens = [_screen("a", 0.1, 100), _screen("b", 0.1, 100)]
        self.assertEqual(set(_labels(pareto_frontier(screens))), {"a", "b"})

    def test_frontier_preserves_input_order(self):
        screens = [_screen("sharp", 0.1, 100), _screen("fast", 0.3, 300)]
        self.assertEqual(_labels(pareto_frontier(screens)), ["sharp", "fast"])


class FinalistTest(unittest.TestCase):
    def test_returns_all_when_frontier_is_small(self):
        frontier = [_screen("a", 0.1, 100), _screen("b", 0.2, 200)]
        self.assertEqual(len(select_finalists(frontier, count=3)), 2)

    def test_spans_the_extremes(self):
        frontier = [_screen(str(i), 0.5 - i * 0.05, 100 + i * 50) for i in range(6)]
        picked = select_finalists(frontier, count=3)
        nps = [s.nps for s in picked]
        # Both extremes of the NPS range are chosen, plus a middle.
        self.assertEqual(min(nps), 100)
        self.assertEqual(max(nps), 350)
        self.assertEqual(len(picked), 3)

    def test_single_finalist_takes_the_lowest_loss(self):
        frontier = [_screen("fast", 0.3, 300), _screen("sharp", 0.1, 100)]
        self.assertEqual(select_finalists(frontier, count=1)[0].candidate.factor, "sharp")

    def test_duplicate_points_collapse(self):
        frontier = [_screen("a", 0.1, 100), _screen("b", 0.1, 100), _screen("c", 0.2, 200)]
        picked = select_finalists(frontier, count=3)
        self.assertEqual(len({(s.val_loss, s.nps) for s in picked}), len(picked))

    def test_zero_finalists_is_rejected(self):
        with self.assertRaises(SweepError):
            select_finalists([_screen("a", 0.1, 100)], count=0)


class SprtCommandTest(unittest.TestCase):
    def test_command_pits_the_candidate_against_the_default(self):
        screen = Screen(
            candidate=Candidate(factor="width", arch=Architecture(hidden=512)),
            val_loss=0.1, nps=1_000_000, param_hash="0xabc", net_path="/nets/cand.sbnn",
        )
        sprt = SprtConfig(
            engine="/bin/seaborg", baseline_net="/nets/default.sbnn",
            baseline_id="gen-002", build_settings="cargo build --release",
            limit="tc=10+0.1", elo0=0.0, elo1=5.0, max_games=40000,
        )
        command = sprt_command(screen, sprt, "/out/sprt")
        self.assertIn("--baseline-option EvalFile=/nets/default.sbnn", command)
        self.assertIn("--candidate-option EvalFile=/nets/cand.sbnn", command)
        self.assertIn("--limit tc=10+0.1", command)
        self.assertIn("--elo0 0.0", command)
        self.assertIn("--elo1 5.0", command)
        self.assertIn("param_hash=0xabc", command)
        self.assertIn(f"/out/sprt/{screen.name}", command)


class ReadParamHashTest(unittest.TestCase):
    def test_reads_the_hash_from_the_sbnn_header(self):
        # A minimal 64-byte header with a known value at the param-hash offset (32).
        header = bytearray(64)
        header[32:40] = (0x1122334455667788).to_bytes(8, "little")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "net.sbnn"
            path.write_bytes(bytes(header) + b"\x00" * 8)
            self.assertEqual(read_param_hash(path), "0x1122334455667788")


class RunSweepTest(unittest.TestCase):
    def test_end_to_end_with_fake_runners(self):
        # A tiny grid so the fake sweep is quick and its frontier is easy to reason
        # about.
        grid = SweepGrid(
            widths=(128, 256), activations=("crelu",), reference_stack=(16,),
            reference_buckets=4, bucket_counts=(4,), stack_depths=((16,),),
            tail_scale_factors=(),
        )
        candidates = enumerate_candidates(grid)
        # Deterministic fake metrics: wider nets are sharper (lower loss) but slower.
        losses = {c.name: 0.5 - 0.01 * c.arch.hidden / 128 for c in candidates}
        nps = {c.name: 1_000_000 - 100 * c.arch.hidden for c in candidates}

        def train_and_export(candidate):
            return losses[candidate.name], Path(f"/nets/{candidate.name}.sbnn"), "0xdead"

        def measure(net_path):
            name = Path(net_path).stem
            return nps[name]

        with tempfile.TemporaryDirectory() as tmp:
            config = SweepConfig(
                training=TrainingConfig(corpus=Path("corpus.bin"), manifest=Path(".")),
                sprt=SprtConfig(
                    engine="/bin/seaborg", baseline_net="/nets/default.sbnn",
                    baseline_id="gen-002", build_settings="cargo build --release",
                ),
                out_dir=Path(tmp), grid=grid, finalists=2, binary_commit="abc123",
            )
            report = run_sweep(config, train_and_export, measure, log=lambda *_: None)

            # Every candidate was screened and attributed.
            self.assertEqual(len(report["candidates"]), len(candidates))
            for record in report["candidates"]:
                self.assertEqual(record["binary_commit"], "abc123")
                self.assertEqual(record["param_hash"], "0xdead")
            # The report is written to disk and reloads as the same object.
            written = json.loads((Path(tmp) / "sweep.json").read_text())
            self.assertEqual(written["frontier"], report["frontier"])
            # One SPRT command per finalist, each naming the strength harness.
            self.assertEqual(len(report["sprt_commands"]), len(report["finalists"]))
            for command in report["sprt_commands"]:
                self.assertIn("strength_test.py", command)

    def test_a_failed_candidate_is_skipped_not_fatal(self):
        grid = SweepGrid(
            widths=(128, 256), activations=("crelu",), bucket_counts=(),
            stack_depths=(), tail_scale_factors=(),
        )

        def train_and_export(candidate):
            if candidate.arch.hidden == 128:
                return None  # simulate a training failure
            return 0.2, Path(f"/nets/{candidate.name}.sbnn"), "0xbeef"

        with tempfile.TemporaryDirectory() as tmp:
            config = SweepConfig(
                training=TrainingConfig(corpus=Path("corpus.bin"), manifest=Path(".")),
                sprt=SprtConfig(
                    engine="/bin/seaborg", baseline_net="/nets/default.sbnn",
                    baseline_id="gen-002", build_settings="build",
                ),
                out_dir=Path(tmp), grid=grid, finalists=3,
            )
            report = run_sweep(config, train_and_export, lambda _: 1000.0, log=lambda *_: None)
            names = [r["name"] for r in report["candidates"]]
            self.assertTrue(all("h128" not in n for n in names))
            self.assertTrue(any("h256" in n for n in names))


if __name__ == "__main__":
    unittest.main()
