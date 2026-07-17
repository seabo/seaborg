import argparse
import tempfile
import unittest
from pathlib import Path

import strength_test as st


PASS_LOG = """Score of candidate vs baseline: 5 - 1 - 4 [0.700] 10
candidate 12.3 8.4 10 70.0 40.0
Ptnml(0-2): 0, 1, 2, 2, 0
SPRT: llr 3.1 (105%), lbound -2.944, ubound 2.944
Finished match
"""
FAIL_LOG = """Score of candidate vs baseline: 1 - 5 - 4 [0.300] 10
SPRT: llr -3.1 (-105%), lbound -2.944, ubound 2.944
Finished match
"""
OPEN_LOG = """Score of candidate vs baseline: 2 - 2 - 6 [0.500] 10
SPRT: llr 0.1 (3%), lbound -2.944, ubound 2.944
Finished match
"""


class StrengthTestTests(unittest.TestCase):
    def config(self):
        return argparse.Namespace(
            runner="cutechess-cli", candidate=Path("/candidate"),
            baseline=Path("/baseline"), time_control="10+0.1", hash_mb=64,
            threads=1, engine_option=["Ponder=false"], max_games=100,
            openings=Path("/openings.epd"), concurrency=2, elo0=-5.0,
            elo1=0.0, alpha=0.05, beta=0.05)

    def test_command_is_paired_equal_and_deterministic(self):
        command = st.build_command(self.config(), Path("games.pgn"))
        self.assertEqual(command.count("-engine"), 2)
        self.assertIn("restart=on", command)
        self.assertIn("option.Threads=1", command)
        self.assertIn("option.Hash=64", command)
        self.assertEqual(command[command.index("-repeat") + 1], "2")
        self.assertIn("policy=round", command)
        self.assertEqual(command[command.index("-rounds") + 1], "100")

    def test_parse_complete_result(self):
        result = st.parse_result(PASS_LOG)
        self.assertEqual((result.wins, result.losses, result.draws), (5, 1, 4))
        self.assertEqual(result.pentanomial, [0, 1, 2, 2, 0])
        self.assertEqual(result.elo, 12.3)

    def test_verdict_and_exit_mappings(self):
        self.assertEqual(st.verdict(st.parse_result(PASS_LOG), 10, True), "PASS")
        self.assertEqual(st.verdict(st.parse_result(FAIL_LOG), 10, True), "FAIL")
        self.assertEqual(st.verdict(st.parse_result(OPEN_LOG), 10, True), "INCONCLUSIVE")
        self.assertEqual(st.VERDICT_EXIT,
                         {"PASS": 0, "FAIL": 1, "INCONCLUSIVE": 2,
                          "INFRASTRUCTURE ERROR": 3})

    def test_smoke_never_passes_authoritative_gate(self):
        self.assertEqual(st.verdict(st.parse_result(PASS_LOG), 10, False),
                         "INCONCLUSIVE")

    def test_cap_does_not_pass_open_sprt(self):
        result = st.parse_result(OPEN_LOG)
        self.assertEqual(result.games, 10)
        self.assertEqual(st.verdict(result, 10, True), "INCONCLUSIVE")

    def test_malformed_incomplete_and_crash_fail_closed(self):
        for output in ("Finished match\n", PASS_LOG.replace("Finished match\n", ""),
                       PASS_LOG + "Engine disconnects\n"):
            with self.subTest(output=output):
                with self.assertRaises(st.InfrastructureError):
                    result = st.parse_result(output)
                    st.verdict(result, 10, True)

    def test_parameter_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = root / "engine"
            engine.write_bytes(b"#!/bin/sh\n")
            engine.chmod(0o755)
            openings = root / "openings"
            openings.write_bytes(b"changed")
            args = argparse.Namespace(
                elo0=0.0, elo1=-5.0, max_games=3, mode="authoritative",
                engine_option=["bad"], baseline=engine, candidate=engine,
                openings=openings)
            with self.assertRaises(st.InfrastructureError):
                st.validate(args)


if __name__ == "__main__":
    unittest.main()
