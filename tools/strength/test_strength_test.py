import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import strength_test as st


# --- Real FastChess output captured from a live v1.5.0 (1eedf82) run of
# seaborg vs seaborg, adjusted only where noted. See docs/strength-testing.md.

# A clean, completed match: 16 games, all legal, SPRT still open -> INCONCLUSIVE.
CLEAN_LOG = """--------------------------------------------------
Results of candidate vs baseline (4 plies, NULL, 16MB, openings-v1.epd):
Elo: -0.00 +/- 0.00, nElo: nan +/- nan
LOS: nan %, DrawRatio: 100.00 %, PairsRatio: nan
Games: 16, Wins: 4, Losses: 4, Draws: 8, Points: 8.0 (50.00 %)
Ptnml(0-2): [0, 0, 8, 0, 0], WL/DD Ratio: 1.00
LLR: 0.00 (0.1%) (-2.94, 2.94) [-5.00, 0.00]
--------------------------------------------------
Finished match
"""

# FastChess warns "Illegal PV move" for a bad principal-variation line while the
# game finishes normally; this must NOT be read as a game failure.
CLEAN_WITH_PV_WARNING_LOG = """Started game 1 of 16 (candidate vs baseline)
Warning; Illegal PV move - move c5d3 from baseline
Finished game 1 (candidate vs baseline): 1-0 {White mates}
""" + CLEAN_LOG

# Real format, wins/LLR raised so the upper boundary is crossed -> PASS.
PASS_LOG = """Finished game 200 (candidate vs baseline): 1-0 {White mates}
--------------------------------------------------
Results of candidate vs baseline (10+0.1, NULL, 16MB, openings-v1.epd):
Elo: 48.00 +/- 21.00, nElo: 33.00 +/- 14.00
LOS: 99.90 %, DrawRatio: 30.00 %, PairsRatio: 3.50
Games: 200, Wins: 96, Losses: 44, Draws: 60, Points: 126.0 (63.00 %)
Ptnml(0-2): [1, 6, 30, 40, 23], WL/DD Ratio: 2.10
LLR: 2.95 (100.3%) (-2.94, 2.94) [-5.00, 0.00]
--------------------------------------------------
Finished match
"""

# Real format, losses/LLR lowered so the lower boundary is crossed -> FAIL.
FAIL_LOG = """--------------------------------------------------
Results of candidate vs baseline (10+0.1, NULL, 16MB, openings-v1.epd):
Elo: -48.00 +/- 21.00, nElo: -33.00 +/- 14.00
LOS: 0.10 %, DrawRatio: 30.00 %, PairsRatio: 0.30
Games: 200, Wins: 44, Losses: 96, Draws: 60, Points: 74.0 (37.00 %)
Ptnml(0-2): [23, 40, 30, 6, 1], WL/DD Ratio: 0.48
LLR: -2.95 (-100.3%) (-2.94, 2.94) [-5.00, 0.00]
--------------------------------------------------
Finished match
"""

# Real illegal-move forfeit (engine played 0000 under a starved clock).
ILLEGAL_LOG = """Started game 2 of 4 (baseline vs candidate)
Warning; Illegal move 0000 played by baseline
Finished game 2 (baseline vs candidate): 0-1 {White makes an illegal move}
--------------------------------------------------
Results of candidate vs baseline (2+0.05, NULL, 16MB, openings-v1.epd):
Games: 4, Wins: 2, Losses: 2, Draws: 0, Points: 2.0 (50.00 %)
LLR: 0.00 (0.0%) (-2.94, 2.94) [-5.00, 0.00]
--------------------------------------------------
Finished match
"""

# Real time-loss forfeit with a nonzero Timeouts summary.
TIMEOUT_LOG = """Finished game 1 (candidate vs baseline): 0-1 {White loses on time}
--------------------------------------------------
Results of candidate vs baseline (2+0.05, NULL, NULL, openings-v1.epd):
Games: 8, Wins: 4, Losses: 4, Draws: 0, Points: 4.0 (50.00 %)
Ptnml(0-2): [0, 0, 4, 0, 0], WL/DD Ratio: inf
LLR: 0.00 (0.0%) (-2.94, 2.94) [-5.00, 0.00]
--------------------------------------------------

Player: candidate
  Timeouts: 4
  Crashed: 0

Finished match
"""

_FASTCHESS = shutil.which(os.environ.get("SEABORG_FASTCHESS", "fastchess"))


class StrengthTestTests(unittest.TestCase):
    def config(self):
        return argparse.Namespace(
            runner="fastchess", candidate=Path("/candidate"),
            baseline=Path("/baseline"), limit="tc=10+0.1", hash_mb=64,
            threads=1, engine_arg=["-u"], engine_option=["Ponder=false"],
            baseline_option=[], candidate_option=[],
            max_games=100, openings=Path("/openings.epd"), concurrency=2,
            elo0=-5.0, elo1=0.0, alpha=0.05, beta=0.05)

    def test_command_is_paired_equal_and_deterministic(self):
        command = st.build_command(self.config(), Path("games.pgn"))
        self.assertEqual(command.count("-engine"), 2)
        self.assertIn("proto=uci", command)
        self.assertIn("tc=10+0.1", command)
        self.assertIn("option.Threads=1", command)
        self.assertIn("option.Hash=64", command)
        self.assertIn("order=sequential", command)
        self.assertEqual(command[command.index("-pgnout") + 1], "file=games.pgn")

    def test_command_plays_each_opening_as_colour_reversed_pair(self):
        config = self.config()
        command = st.build_command(config, Path("games.pgn"))
        rounds = int(command[command.index("-rounds") + 1])
        games = int(command[command.index("-games") + 1])
        repeat = int(command[command.index("-repeat") + 1])
        # Each opening is played twice with reversed colours.
        self.assertEqual(games, 2)
        self.assertEqual(repeat, 2)
        # Rounds count pairs and the total game budget matches max_games.
        self.assertEqual(rounds, config.max_games // 2)
        self.assertEqual(rounds * games, config.max_games)

    def test_command_applies_engine_args_to_both_engines(self):
        command = st.build_command(self.config(), Path("games.pgn"))
        self.assertEqual(command.count("args=-u"), 2)

    def test_per_side_options_go_to_the_right_engine_only(self):
        # The gate tells one binary apart from itself by giving each side its own network. Per-side
        # options must land in that engine's own block, never in the shared -each block that would
        # apply them to both.
        config = self.config()
        config.baseline_option = ["EvalFile=best.sbnn"]
        config.candidate_option = ["EvalFile=candidate.sbnn"]
        command = st.build_command(config, Path("games.pgn"))

        def block_for(name):
            start = next(i for i, tok in enumerate(command)
                         if tok == "-engine" and command[i + 1] == f"name={name}")
            end = next((i for i in range(start + 1, len(command))
                        if command[i] in ("-engine", "-each")), len(command))
            return command[start:end]

        self.assertIn("option.EvalFile=candidate.sbnn", block_for("candidate"))
        self.assertNotIn("option.EvalFile=best.sbnn", block_for("candidate"))
        self.assertIn("option.EvalFile=best.sbnn", block_for("baseline"))
        self.assertNotIn("option.EvalFile=candidate.sbnn", block_for("baseline"))
        # Neither per-side option leaks into the shared block.
        each = command[command.index("-each"):]
        self.assertNotIn("option.EvalFile=best.sbnn", each)
        self.assertNotIn("option.EvalFile=candidate.sbnn", each)

    def test_command_restarts_engine_processes_between_games(self):
        command = st.build_command(self.config(), Path("games.pgn"))
        # restart=on must sit inside the -each per-engine options so FastChess
        # actually restarts both engines between games (default is off).
        each_index = command.index("-each")
        following = command[each_index + 1:]
        each_options = following[:next(
            (i for i, token in enumerate(following) if token.startswith("-")),
            len(following))]
        self.assertIn("restart=on", each_options)

    def test_parse_complete_result(self):
        result = st.parse_result(CLEAN_LOG)
        self.assertEqual((result.games, result.wins, result.losses, result.draws),
                         (16, 4, 4, 8))
        self.assertEqual(result.pentanomial, [0, 0, 8, 0, 0])
        self.assertEqual(result.elo, 0.0)
        self.assertTrue(result.runner_finished)

    def test_illegal_pv_move_warning_is_not_a_failure(self):
        result = st.parse_result(CLEAN_WITH_PV_WARNING_LOG)
        self.assertEqual(result.games, 16)
        self.assertEqual(st.verdict(result, 100, True), "INCONCLUSIVE")

    def test_verdict_and_exit_mappings(self):
        self.assertEqual(st.verdict(st.parse_result(PASS_LOG), 200, True), "PASS")
        self.assertEqual(st.verdict(st.parse_result(FAIL_LOG), 200, True), "FAIL")
        self.assertEqual(st.verdict(st.parse_result(CLEAN_LOG), 16, True), "INCONCLUSIVE")
        self.assertEqual(st.VERDICT_EXIT,
                         {"PASS": 0, "FAIL": 1, "INCONCLUSIVE": 2,
                          "INFRASTRUCTURE ERROR": 3})

    def test_smoke_never_passes_authoritative_gate(self):
        self.assertEqual(st.verdict(st.parse_result(PASS_LOG), 200, False),
                         "INCONCLUSIVE")

    def test_cap_does_not_pass_open_sprt(self):
        result = st.parse_result(CLEAN_LOG)
        self.assertEqual(result.games, 16)
        self.assertEqual(st.verdict(result, 16, True), "INCONCLUSIVE")

    def test_over_cap_is_infrastructure_error(self):
        result = st.parse_result(CLEAN_LOG)
        with self.assertRaises(st.InfrastructureError):
            st.verdict(result, 10, True)

    def test_malformed_incomplete_and_forfeit_fail_closed(self):
        for output in ("malformed runner output\n",
                       CLEAN_LOG.replace("Finished match\n", ""),
                       ILLEGAL_LOG, TIMEOUT_LOG):
            with self.subTest(output=output):
                with self.assertRaises(st.InfrastructureError):
                    result = st.parse_result(output)
                    st.verdict(result, 200, True)

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
                limit="tc=10+0.1", engine_option=["bad"], baseline=engine,
                candidate=engine, openings=openings)
            with self.assertRaises(st.InfrastructureError):
                st.validate(args)

    def test_authoritative_rejects_non_time_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = root / "engine"
            engine.write_bytes(b"#!/bin/sh\n")
            engine.chmod(0o755)
            args = argparse.Namespace(
                elo0=-5.0, elo1=0.0, max_games=100, mode="authoritative",
                limit="depth=4", engine_option=[], baseline=engine,
                candidate=engine, openings=root / "missing.epd")
            with self.assertRaises(st.InfrastructureError) as caught:
                st.validate(args)
            self.assertIn("time-based limit", str(caught.exception))

    def test_real_cli_missing_arguments_is_infrastructure_error(self):
        proc = subprocess.run(
            [sys.executable, str(Path(st.__file__))],
            text=True, capture_output=True, check=False)
        self.assertEqual(proc.returncode, st.INFRA_ERROR)
        self.assertIn("INFRASTRUCTURE ERROR", proc.stdout)
        self.assertNotIn("INCONCLUSIVE", proc.stdout + proc.stderr)

    def test_real_cli_invalid_typed_value_preserves_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "artifacts"
            proc = subprocess.run(
                [sys.executable, str(Path(st.__file__)), "--output", str(output),
                 "--max-games", "not-a-number"],
                text=True, capture_output=True, check=False)
            self.assertEqual(proc.returncode, st.INFRA_ERROR)
            self.assertIn("INFRASTRUCTURE ERROR", proc.stdout)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["verdict"], "INFRASTRUCTURE ERROR")
            self.assertIn("invalid command line", report["error"])

    def run_with_mocks(self, runner_output, runner_exit=0, extra_argv=(),
                       run_side_effect=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "artifacts"
            baseline = root / "baseline"
            candidate = root / "candidate"
            argv = [
                "--baseline", str(baseline), "--baseline-id", "base-sha",
                "--candidate", str(candidate), "--candidate-id", "candidate-sha",
                "--build-settings", "cargo build --release",
                "--output", str(output), *extra_argv,
            ]
            completed = subprocess.CompletedProcess(
                ["fastchess"], runner_exit, runner_output, "")

            def fake_run(*args, **kwargs):
                if run_side_effect is not None:
                    raise run_side_effect
                return completed

            with mock.patch.object(st, "validate"), \
                    mock.patch.object(st, "runner_version", return_value="fastchess 1.5.0"), \
                    mock.patch.object(st, "uci_preflight", return_value={"bestmove": "e2e4"}), \
                    mock.patch.object(st, "sha256", return_value="0" * 64), \
                    mock.patch.object(st.subprocess, "run", side_effect=fake_run), \
                    mock.patch("builtins.print") as printed:
                exit_code = st.run(argv)
            report = json.loads((output / "report.json").read_text())
            return exit_code, report, printed

    def assert_run_failure(self, runner_output, runner_exit=0):
        exit_code, report, printed = self.run_with_mocks(runner_output, runner_exit)
        self.assertEqual(exit_code, st.INFRA_ERROR)
        self.assertTrue(any("INFRASTRUCTURE ERROR" in str(call)
                            for call in printed.call_args_list))
        self.assertEqual(report["verdict"], "INFRASTRUCTURE ERROR")
        self.assertIn("error", report)

    def test_run_success_path_passes_and_records_results(self):
        exit_code, report, printed = self.run_with_mocks(PASS_LOG, runner_exit=0)
        self.assertEqual(exit_code, st.PASS)
        self.assertEqual(report["verdict"], "PASS")
        self.assertEqual(report["authority"], "AUTHORITATIVE")
        self.assertEqual(report["runner_exit_code"], 0)
        # SPRT likelihood state and result statistics are populated end-to-end.
        self.assertEqual(report["results"]["games"], 200)
        self.assertEqual(
            (report["results"]["wins"], report["results"]["draws"],
             report["results"]["losses"]),
            (96, 60, 44))
        for field in ("llr", "lower_bound", "upper_bound"):
            self.assertIn(field, report["sprt"])
        self.assertTrue(any("verdict: PASS" in str(call)
                            for call in printed.call_args_list))

    def test_run_malformed_and_incomplete_output_are_infrastructure_errors(self):
        for output in ("malformed runner output\n",
                       CLEAN_LOG.replace("Finished match\n", "")):
            with self.subTest(output=output):
                self.assert_run_failure(output)

    def test_run_forfeit_and_nonzero_runner_are_infrastructure_errors(self):
        self.assert_run_failure(ILLEGAL_LOG)
        self.assert_run_failure(TIMEOUT_LOG)
        self.assert_run_failure(PASS_LOG, runner_exit=7)

    def test_match_timeout_fails_closed(self):
        # TimeoutExpired.output is undecoded bytes even under text=True.
        timeout = subprocess.TimeoutExpired("fastchess", timeout=1,
                                            output=b"partial log\n")
        exit_code, report, _ = self.run_with_mocks(
            "unused", extra_argv=["--match-timeout", "1"], run_side_effect=timeout)
        self.assertEqual(exit_code, st.INFRA_ERROR)
        self.assertIn("match-timeout", report["error"])

    # Exercise the tool's real-runner integration against the actual FastChess
    # binary (no mocking), skipped when it is absent. A live seaborg match or
    # even preflight is intentionally NOT asserted: seaborg self-play is
    # currently unstable and can hang a single search (see backlog TASK-32 /
    # TASK-34), so the parsing/verdict paths are covered by the real captured
    # fixtures above and a full match is verified manually.
    @unittest.skipUnless(_FASTCHESS, "requires fastchess on PATH")
    def test_live_runner_version_reads_real_fastchess(self):
        version = st.runner_version(_FASTCHESS)
        self.assertIn("fastchess", version.lower())


if __name__ == "__main__":
    unittest.main()
