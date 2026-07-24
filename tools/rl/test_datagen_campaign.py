"""Tests for the datagen campaign chaining: distinct opening seed per shard, the
two stop bounds (shard count and sample target), abort-on-failure, and a
provenance manifest that records the labeller, purity, and every parameter. The
engine is never run -- a fake runner writes valid shard files so the chaining
and accumulation logic is exercised on its own."""

from __future__ import annotations

import json
import struct
import tempfile
import unittest
from pathlib import Path

import concat_samples as cs
import datagen_campaign as dc


def _shard_bytes(records: int) -> bytes:
    header = cs.MAGIC + struct.pack("<HH", cs.FORMAT_VERSION, cs.RECORD_SIZE)
    return header + b"\x00" * (records * cs.RECORD_SIZE)


class FakeRunner:
    """Writes a valid shard of a fixed record count and records each command."""

    def __init__(self, records_per_shard: int = 10, fail_at: int | None = None) -> None:
        self.records_per_shard = records_per_shard
        self.fail_at = fail_at
        self.commands: list[list] = []

    def __call__(self, command: list, out: Path) -> int:
        self.commands.append(command)
        if self.fail_at is not None and len(self.commands) - 1 == self.fail_at:
            return 1
        out.write_bytes(_shard_bytes(self.records_per_shard))
        return 0


class CampaignTest(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.network = self.dir / "net.sbnn"
        self.network.write_bytes(b"fake network bytes")

    def _config(self, **overrides) -> dc.CampaignConfig:
        base = dict(
            engine=Path("/bin/seaborg"),
            network=self.network,
            network_generation=2,
            out_dir=self.dir / "out",
            games_per_shard=1000,
        )
        base.update(overrides)
        return dc.CampaignConfig(**base)

    def test_shard_count_bound_stops_and_joins(self):
        runner = FakeRunner(records_per_shard=10)
        manifest = dc.run(self._config(shards=3), runner, log=lambda *_: None)

        self.assertEqual(len(manifest["shards"]), 3)
        self.assertEqual(manifest["total_records"], 30)
        corpus = self.dir / "out" / "corpus.bin"
        cs.verify(corpus, 30)  # the joined corpus is a valid single stream

    def test_distinct_opening_seed_per_shard(self):
        runner = FakeRunner()
        dc.run(self._config(shards=3, base_seed=500), runner, log=lambda *_: None)

        seeds = []
        for command in runner.commands:
            seeds.append(int(command[command.index("--opening-seed") + 1]))
        self.assertEqual(seeds, [500, 501, 502])

    def test_sample_target_bound_stops_once_reached(self):
        runner = FakeRunner(records_per_shard=10)
        # 25 samples wanted at 10/shard: shard 3 crosses the target.
        manifest = dc.run(self._config(target_samples=25), runner, log=lambda *_: None)
        self.assertEqual(len(manifest["shards"]), 3)
        self.assertEqual(manifest["total_records"], 30)

    def test_unbounded_campaign_is_rejected(self):
        with self.assertRaises(ValueError):
            dc.run(self._config(), FakeRunner(), log=lambda *_: None)

    def test_failed_shard_aborts_and_keeps_prior_shards(self):
        runner = FakeRunner(fail_at=2)  # shard index 2 fails
        with self.assertRaises(RuntimeError):
            dc.run(self._config(shards=5), runner, log=lambda *_: None)
        # Shards 0 and 1 were written before the failure; no corpus was joined.
        self.assertTrue((self.dir / "out" / "shard_000.bin").exists())
        self.assertTrue((self.dir / "out" / "shard_001.bin").exists())
        self.assertFalse((self.dir / "out" / "corpus.bin").exists())

    def test_command_selects_network_and_higher_budget(self):
        command = dc.datagen_command(self._config(nodes=25_000), seed=7, out=Path("s.bin"))
        self.assertIn("--network", command)
        self.assertEqual(command[command.index("--network") + 1], str(self.network))
        self.assertEqual(command[command.index("--nodes") + 1], "25000")
        self.assertEqual(command[command.index("--opening-seed") + 1], "7")

    def test_manifest_records_labeller_and_purity(self):
        runner = FakeRunner()
        manifest = dc.run(self._config(shards=1), runner, log=lambda *_: None)

        labeller = manifest["labeller"]
        self.assertEqual(labeller["sha256"], dc.sha256(self.network))
        self.assertTrue(labeller["network_id"].startswith("nnue:gen-002:sha256="))
        self.assertIn("no external", manifest["purity"])
        # The manifest is also written beside the corpus.
        written = json.loads((self.dir / "out" / "corpus.manifest.json").read_text())
        self.assertEqual(written["total_records"], manifest["total_records"])


if __name__ == "__main__":
    unittest.main()
