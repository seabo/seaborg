"""Tests for the reinforcement-loop orchestration.

These exercise the loop's own logic — bootstrap, promotion, attribution, the
purity invariant, and iteration numbering — with a fake backend in place of
datagen, PyTorch training, and the FastChess gate. The external steps are behind
the [`loop.Backend`] seam precisely so this coverage needs none of them.
"""

import json
import tempfile
import unittest
from pathlib import Path

import loop as rl


class FakeBackend(rl.Backend):
    """A backend that records its calls and returns scripted gate verdicts.

    It writes just enough real bytes — a sample file, a checkpoint, and a
    per-generation-unique candidate network — for the orchestrator's file
    accounting (sample count, hashing, promotion copies) to run for real.
    """

    def __init__(self, verdict_for=None, samples=4096):
        # verdict_for(generation) -> one of loop.VERDICT_BY_EXIT's values.
        self.verdict_for = verdict_for or (lambda generation: "PASS")
        self.samples = samples
        self.generate_calls = []
        self.gate_calls = []
        self._exports = 0

    def generate(self, out, network, nodes, games):
        self.generate_calls.append(
            {"out": out, "network": network, "nodes": nodes, "games": games}
        )
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(b"\x00" * (rl.SAMPLE_HEADER_SIZE + rl.SAMPLE_RECORD_SIZE))
        return rl.GenerateResult(path=out, samples=self.samples)

    def train(self, data, checkpoint, generation):
        checkpoint.write_bytes(b"checkpoint")

    def export(self, checkpoint, network):
        # Distinct bytes per call so each generation's network hashes differently.
        self._exports += 1
        network.write_bytes(f"fake-network-{self._exports}".encode())

    def gate(self, baseline_network, candidate_network, output_dir, generation):
        self.gate_calls.append(
            {"baseline": baseline_network, "candidate": candidate_network, "gen": generation}
        )
        verdict = self.verdict_for(generation)
        exit_code = next(code for code, name in rl.VERDICT_BY_EXIT.items() if name == verdict)
        return rl.GateResult(
            verdict=verdict,
            exit_code=exit_code,
            output_dir=output_dir,
            elo=3.5 if verdict == "PASS" else -4.0,
            elo_interval=[0.2, 6.8],
            games_played=200,
        )


class LoopTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.state = Path(self._tmp.name) / "state"

    def tearDown(self):
        self._tmp.cleanup()

    def config(self):
        return rl.LoopConfig(
            state_dir=self.state,
            engine=Path("/engine/seaborg"),
            trainer_dir=Path("/trainer"),
            strength_script=Path("/trainer/strength_test.py"),
            games=200,
            nodes=3_000,
        )

    def ledger_records(self):
        ledger = self.state / rl.LEDGER
        return [json.loads(line) for line in ledger.read_text().splitlines() if line.strip()]

    def test_generation_zero_bootstraps_from_handcrafted_evaluation(self):
        backend = FakeBackend()
        result = rl.ReinforcementLoop(self.config(), backend).run_iteration(0)

        # Datagen and the gate both saw no network: generation 0 plays with the
        # hand-crafted evaluation, on both the data-generation and gate sides.
        self.assertIsNone(backend.generate_calls[0]["network"])
        self.assertIsNone(backend.gate_calls[0]["baseline"])

        record = self.ledger_records()[0]
        self.assertTrue(record["baseline"]["bootstrap"])
        self.assertEqual(record["baseline"]["network_id"], "handcrafted")
        self.assertTrue(result.promoted)

    def test_promotes_and_records_best_only_on_pass(self):
        backend = FakeBackend(lambda generation: "PASS")
        rl.ReinforcementLoop(self.config(), backend).run_iteration(0)

        best = self.state / rl.BEST_NETWORK
        archived = self.state / rl.NETWORKS_DIR / "gen-000.sbnn"
        self.assertTrue(best.is_file())
        self.assertTrue(archived.is_file())
        manifest = json.loads((self.state / rl.BEST_MANIFEST).read_text())
        self.assertEqual(manifest["generation"], 0)
        self.assertTrue(self.ledger_records()[0]["promoted"])

    def test_non_pass_verdicts_do_not_promote(self):
        for verdict in ("FAIL", "INCONCLUSIVE", "INFRASTRUCTURE_ERROR"):
            with self.subTest(verdict=verdict):
                self.tearDown()
                self.setUp()
                backend = FakeBackend(lambda generation, v=verdict: v)
                result = rl.ReinforcementLoop(self.config(), backend).run_iteration(0)

                self.assertFalse(result.promoted)
                # No best network exists, and the failed decision is still recorded.
                self.assertFalse((self.state / rl.BEST_NETWORK).exists())
                record = self.ledger_records()[0]
                self.assertEqual(record["gate"]["verdict"], verdict)
                self.assertFalse(record["promoted"])

    def test_later_generation_plays_with_the_promoted_network(self):
        backend = FakeBackend(lambda generation: "PASS")
        loop = rl.ReinforcementLoop(self.config(), backend)
        loop.run_iteration(0)
        loop.run_iteration(1)

        best = self.state / rl.BEST_NETWORK
        # Generation 1 generated data and gated against the network generation 0
        # promoted — the reinforcement signal is the loop's own prior network.
        self.assertEqual(backend.generate_calls[1]["network"], best)
        self.assertEqual(backend.gate_calls[1]["baseline"], best)

    def test_a_failed_candidate_leaves_the_previous_best_intact(self):
        verdicts = {0: "PASS", 1: "FAIL"}
        backend = FakeBackend(lambda generation: verdicts[generation])
        loop = rl.ReinforcementLoop(self.config(), backend)
        loop.run_iteration(0)
        best = self.state / rl.BEST_NETWORK
        promoted_sha = rl.sha256(best)

        loop.run_iteration(1)
        # The rejected candidate did not overwrite the best pointer.
        self.assertEqual(rl.sha256(best), promoted_sha)
        # And generation 1 still generated its data against that surviving best.
        self.assertEqual(backend.generate_calls[1]["network"], best)

    def test_attribution_records_data_volume_budget_ids_and_delta(self):
        backend = FakeBackend(samples=9001)
        rl.ReinforcementLoop(self.config(), backend).run_iteration(0)
        record = self.ledger_records()[0]

        self.assertEqual(record["data"], {"games": 200, "samples": 9001, "node_budget": 3_000})
        self.assertTrue(record["candidate"]["network_id"].startswith("nnue:gen-000:sha256="))
        self.assertEqual(record["gate"]["elo"], 3.5)
        self.assertEqual(record["gate"]["games_played"], 200)
        self.assertEqual(record["gate"]["verdict"], "PASS")

    def test_evaluator_is_never_external(self):
        # The purity invariant: the only evaluator any step plays with is the
        # hand-crafted default (None) or a network under the run's own state
        # directory, promoted by this loop from earlier self-play. Nothing else.
        backend = FakeBackend(lambda generation: "PASS")
        loop = rl.ReinforcementLoop(self.config(), backend)
        for generation in range(3):
            loop.run_iteration(generation)

        best = self.state / rl.BEST_NETWORK
        for call in backend.generate_calls:
            self.assertIn(call["network"], (None, best))
        for call in backend.gate_calls:
            self.assertIn(call["baseline"], (None, best))

    def test_run_numbers_generations_and_resumes_from_the_ledger(self):
        backend = FakeBackend(lambda generation: "PASS")
        results = rl.ReinforcementLoop(self.config(), backend).run(2)
        self.assertEqual([r.generation for r in results], [0, 1])

        # A fresh loop over the same state continues where the ledger left off.
        resumed = rl.ReinforcementLoop(self.config(), FakeBackend()).run(1)
        self.assertEqual([r.generation for r in resumed], [2])
        self.assertEqual([r["generation"] for r in self.ledger_records()], [0, 1, 2])

    def test_a_broken_step_stops_the_iteration_without_recording_it(self):
        class BrokenTrain(FakeBackend):
            def train(self, data, checkpoint, generation):
                raise rl.LoopError("training blew up")

        loop = rl.ReinforcementLoop(self.config(), BrokenTrain())
        with self.assertRaises(rl.LoopError):
            loop.run_iteration(0)

        # A stopped iteration records nothing and promotes nothing: no half-written
        # ledger entry, no best network.
        self.assertFalse((self.state / rl.LEDGER).exists())
        self.assertFalse((self.state / rl.BEST_NETWORK).exists())


if __name__ == "__main__":
    unittest.main()
