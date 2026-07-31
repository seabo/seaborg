"""Unit tests for the mate-find-rate diagnostic's scoring logic.

These cover the pure functions that turn raw UCI output into a find-rate report:
parsing the engine's answer, the two rules-verified success criteria, and the
per-distance aggregation. They deliberately do not spawn the engine — the engine
side is exercised end to end by running the tool — so they stay fast and
hermetic, matching the other tool tests in this repo.

Run: python3 -m unittest discover -p 'test_*.py'
"""

import argparse
import unittest

import mate_find_rate as mfr


class ParseTest(unittest.TestCase):
    def test_bestmove_is_extracted(self):
        lines = ["info depth 3 score mate 2 pv d1h5\n", "bestmove d1h5 ponder e8e7\n"]
        self.assertEqual(mfr.parse_bestmove(lines), "d1h5")

    def test_null_bestmove_is_none(self):
        self.assertIsNone(mfr.parse_bestmove(["bestmove 0000\n"]))
        self.assertIsNone(mfr.parse_bestmove(["bestmove (none)\n"]))

    def test_missing_bestmove_is_none(self):
        self.assertIsNone(mfr.parse_bestmove(["info depth 1\n"]))

    def test_final_score_takes_the_last_scored_info(self):
        lines = [
            "info depth 1 score cp 30\n",
            "info depth 2 score cp 120\n",
            "info depth 3 score mate 2\n",
            "bestmove d1h5\n",
        ]
        self.assertEqual(mfr.parse_final_score(lines), ("mate", 2))

    def test_final_score_ignores_unscored_lines(self):
        lines = ["info string hello\n", "info depth 2 score cp -45\n", "bestmove a2a3\n"]
        self.assertEqual(mfr.parse_final_score(lines), ("cp", -45))

    def test_final_score_none_when_absent(self):
        self.assertIsNone(mfr.parse_final_score(["bestmove a2a3\n"]))

    def test_reports_mate_only_for_positive_mate(self):
        self.assertTrue(mfr.reports_mate(("mate", 3)))
        self.assertFalse(mfr.reports_mate(("mate", -3)))
        self.assertFalse(mfr.reports_mate(("cp", 900)))
        self.assertFalse(mfr.reports_mate(None))


class EvaluateTest(unittest.TestCase):
    POSITION = {
        "fen": "1Q6/6K1/8/8/8/8/Q7/2k5 w - - 1 62",
        "mate_moves": 1,
        "winning_first_moves": ["b8b1", "a2a1"],
    }

    def test_playing_a_winning_move_and_seeing_the_mate(self):
        outcome = mfr.evaluate(self.POSITION, "b8b1", ("mate", 1))
        self.assertTrue(outcome.played_mating_move)
        self.assertTrue(outcome.reported_mate)
        self.assertEqual(outcome.mate_moves, 1)

    def test_non_winning_move_is_not_solved(self):
        outcome = mfr.evaluate(self.POSITION, "g7g6", ("cp", 500))
        self.assertFalse(outcome.played_mating_move)
        self.assertFalse(outcome.reported_mate)

    def test_report_and_play_are_independent(self):
        # A pruned search can play the mating move without ever scoring it a mate.
        outcome = mfr.evaluate(self.POSITION, "b8b1", ("cp", 900))
        self.assertTrue(outcome.played_mating_move)
        self.assertFalse(outcome.reported_mate)

    def test_no_bestmove_is_not_solved(self):
        outcome = mfr.evaluate(self.POSITION, None, ("mate", 1))
        self.assertFalse(outcome.played_mating_move)
        self.assertTrue(outcome.reported_mate)


class AggregateTest(unittest.TestCase):
    def test_buckets_and_rates(self):
        outcomes = [
            mfr.Outcome(mate_moves=1, played_mating_move=True, reported_mate=True),
            mfr.Outcome(mate_moves=1, played_mating_move=True, reported_mate=False),
            mfr.Outcome(mate_moves=3, played_mating_move=False, reported_mate=False),
            mfr.Outcome(mate_moves=3, played_mating_move=True, reported_mate=True),
        ]
        agg = mfr.aggregate(outcomes)

        by_distance = {row["mate_moves"]: row for row in agg["by_distance"]}
        self.assertEqual(by_distance[1]["total"], 2)
        self.assertEqual(by_distance[1]["play_rate"], 1.0)
        self.assertEqual(by_distance[1]["report_rate"], 0.5)
        self.assertEqual(by_distance[3]["play_rate"], 0.5)
        self.assertEqual(by_distance[3]["report_rate"], 0.5)

        self.assertEqual(agg["overall"]["total"], 4)
        self.assertEqual(agg["overall"]["play_rate"], 0.75)
        self.assertEqual(agg["overall"]["report_rate"], 0.5)

    def test_distances_are_sorted(self):
        outcomes = [
            mfr.Outcome(mate_moves=4, played_mating_move=True, reported_mate=True),
            mfr.Outcome(mate_moves=2, played_mating_move=True, reported_mate=True),
        ]
        agg = mfr.aggregate(outcomes)
        self.assertEqual([r["mate_moves"] for r in agg["by_distance"]], [2, 4])

    def test_empty_is_zero_rates(self):
        agg = mfr.aggregate([])
        self.assertEqual(agg["overall"]["total"], 0)
        self.assertEqual(agg["overall"]["play_rate"], 0.0)


class BuildGoTest(unittest.TestCase):
    def _args(self, **kw):
        base = {"movetime": 200, "depth": 0, "nodes": 0}
        base.update(kw)
        return argparse.Namespace(**base)

    def test_movetime_is_default(self):
        go, desc = mfr.build_go(self._args())
        self.assertEqual(go, "go movetime 200")
        self.assertIn("200ms", desc)

    def test_depth_takes_precedence(self):
        go, _ = mfr.build_go(self._args(depth=12))
        self.assertEqual(go, "go depth 12")

    def test_nodes_when_no_depth(self):
        go, _ = mfr.build_go(self._args(nodes=500000))
        self.assertEqual(go, "go nodes 500000")


if __name__ == "__main__":
    unittest.main()
