"""Tests for quantization and the SBNN export.

They pin down the things export must get right: the quantization rounds each weight
onto the engine's integer grid, the serialized bytes are the exact format the
engine loader reads (checked with a reader written independently of the writer,
mirroring the loader's rejections), the exported integer network reproduces the
trained model's own evaluation to within the final rounding step, and the
golden-vector fixture the engine's differential test consumes is emitted from a FEN
the same way the engine derives its features -- the cross-language sync check."""

from __future__ import annotations

import unittest
from pathlib import Path

import numpy as np
import torch

import data
import export
import train
from export import (
    GOLDEN_POSITIONS,
    ExportError,
    QuantizedNetwork,
    features_from_fen,
    golden_vectors,
    integer_eval_cp,
    quantize,
)
from model import PERSPECTIVE_768_DIM, NnueConfig, NnueModel
from testsupport import (
    BLACK_KING,
    BLACK_PAWN,
    BLACK_ROOK,
    WHITE_KING,
    WHITE_PAWN,
    WHITE_QUEEN,
    WHITE_ROOK,
    encode_record,
)

# The committed golden fixture the engine's differential test loads; this suite
# checks it stays in sync with what the exporter currently emits.
_GOLDEN_VECTORS_PATH = (
    Path(__file__).resolve().parents[2] / "engine" / "tests" / "fixtures" / "golden_v1.vectors"
)

# The exported integer network reproduces the quantization-aware model's own
# centipawn output to within the dequantizing divide's rounding: with the same
# rounded weights and activations on both sides, integer_eval_cp equals
# round(SCALE * fout), so the only gap is that final round-half-away-from-zero,
# bounded by 0.5cp. One centipawn leaves a hair of margin for float error.
_REPRODUCTION_TOLERANCE_CP = 1.0


def _model_with_weights(config, w_ft, b_ft, w_out, b_out) -> NnueModel:
    """A model whose parameters are set to given float tensors, so a quantization
    result can be checked against a hand computation."""
    model = NnueModel(config)
    with torch.no_grad():
        model.feature_transformer.weight.copy_(torch.as_tensor(w_ft, dtype=torch.float32))
        model.ft_bias.copy_(torch.as_tensor(b_ft, dtype=torch.float32))
        model.output.weight.copy_(torch.as_tensor(w_out, dtype=torch.float32))
        model.output.bias.copy_(torch.as_tensor(b_out, dtype=torch.float32))
    return model


class QuantizationTest(unittest.TestCase):
    def test_weights_round_onto_the_contract_grids(self):
        config = NnueConfig(hidden=16)
        h = config.hidden
        # Distinct fractional weights so the rounding is observable per block. The
        # model stores parameters in float32, so the expectation rounds the float32
        # values the exporter actually sees, not the float64 originals.
        w_ft = np.linspace(-0.4, 0.4, PERSPECTIVE_768_DIM * h).reshape(PERSPECTIVE_768_DIM, h)
        b_ft = np.linspace(-0.1, 0.1, h)
        w_out = np.linspace(-0.5, 0.5, 2 * h).reshape(1, 2 * h)
        b_out = np.array([0.037])
        net = quantize(_model_with_weights(config, w_ft, b_ft, w_out, b_out))

        def grid(values, scale):
            return np.rint(np.asarray(values, np.float32).astype(np.float64) * scale)

        # W_ft is feature-major: row f of [768, H] lands contiguously at f*H.
        np.testing.assert_array_equal(net.w_ft, grid(w_ft, config.qa).reshape(-1))
        np.testing.assert_array_equal(net.b_ft, grid(b_ft, config.qa))
        np.testing.assert_array_equal(net.w_out, grid(w_out.reshape(-1), config.qb))
        np.testing.assert_array_equal(net.b_out, grid(b_out, config.qa * config.qb))
        self.assertEqual(net.w_ft.dtype, np.int16)
        self.assertEqual(net.b_out.dtype, np.int32)

    def test_round_half_to_even(self):
        # Exact halves round to the nearest even integer -- banker's rounding, the
        # contract-fixed mode. Checked on the rounding helper directly so float32
        # parameter storage cannot perturb a value off the .5 boundary.
        rounded = export._round_half_even(np.array([0.5, 1.5, 2.5, 3.5, -0.5, -1.5]), scale=1.0)
        np.testing.assert_array_equal(rounded, [0.0, 2.0, 2.0, 4.0, 0.0, -2.0])
        # The scale multiplies before rounding: 0.25 and 0.75 at scale 10 are the
        # exact halves 2.5 and 7.5, rounding to the nearest even, 2 and 8.
        np.testing.assert_array_equal(
            export._round_half_even(np.array([0.25, 0.75]), scale=10.0), [2.0, 8.0]
        )

    def test_overflowing_output_weight_is_rejected(self):
        config = NnueConfig(hidden=16)
        h = config.hidden
        w_out = np.zeros((1, 2 * h))
        w_out[0, 0] = 1000.0  # 1000 * QB = 64000, past i16::MAX
        with self.assertRaises(ExportError):
            quantize(
                _model_with_weights(
                    config,
                    np.zeros((PERSPECTIVE_768_DIM, h)),
                    np.zeros(h),
                    w_out,
                    np.array([0.0]),
                )
            )


class AccumulatorBoundTest(unittest.TestCase):
    def _net(self, w_ft, b_ft, hidden=16) -> QuantizedNetwork:
        return QuantizedNetwork(
            hidden=hidden,
            qa=255,
            qb=64,
            scale=400,
            w_ft=np.asarray(w_ft, dtype=np.int16),
            b_ft=np.asarray(b_ft, dtype=np.int16),
            w_out=np.zeros(2 * hidden, dtype=np.int16),
            b_out=np.zeros(1, dtype=np.int32),
        )

    def test_bounded_weights_pass(self):
        h = 16
        # 32 features of magnitude 200 plus a small bias: 32*200 + 10 = 6410 < i16.
        w_ft = np.full((PERSPECTIVE_768_DIM, h), 200, dtype=np.int16)
        export._assert_accumulator_fits_i16(self._net(w_ft.reshape(-1), np.full(h, 10)))

    def test_overflowing_accumulator_is_rejected(self):
        h = 16
        # 32 same-signed columns of 1100 already exceed i16::MAX (35200 > 32767).
        w_ft = np.full((PERSPECTIVE_768_DIM, h), 1100, dtype=np.int16)
        with self.assertRaises(ExportError):
            export._assert_accumulator_fits_i16(self._net(w_ft.reshape(-1), np.zeros(h)))

    def test_only_the_largest_columns_count(self):
        # A single unit with 33 large weights: only the 32 largest may contribute
        # (at most 32 pieces), so a 33rd large weight must not tip it over.
        h = 16
        columns = np.zeros((PERSPECTIVE_768_DIM, h), dtype=np.int64)
        columns[:33, 0] = 1000  # 33 large weights in unit 0
        # 32 * 1000 = 32000 < i16::MAX; the 33rd is excluded, so this passes.
        export._assert_accumulator_fits_i16(
            self._net(columns.astype(np.int16).reshape(-1), np.zeros(h))
        )


class SerializationTest(unittest.TestCase):
    """The serialized bytes are the format the engine loader reads. ``from_bytes``
    is an independent reader (it does not call ``to_bytes``), so a round trip
    exercises the byte layout from both directions."""

    def _demo(self) -> QuantizedNetwork:
        return export._demo_network(hidden=16)

    def test_round_trips_to_identical_weights_and_metadata(self):
        net = self._demo()
        reloaded = QuantizedNetwork.from_bytes(net.to_bytes())
        self.assertEqual((reloaded.hidden, reloaded.qa, reloaded.qb, reloaded.scale), (16, 255, 64, 400))
        np.testing.assert_array_equal(reloaded.w_ft, net.w_ft)
        np.testing.assert_array_equal(reloaded.b_ft, net.b_ft)
        np.testing.assert_array_equal(reloaded.w_out, net.w_out)
        np.testing.assert_array_equal(reloaded.b_out, net.b_out)

    def test_header_fields_land_at_the_contract_offsets(self):
        raw = self._demo().to_bytes()
        self.assertEqual(raw[:4], export.MAGIC)
        self.assertEqual(len(raw), export.HEADER_LEN + self._demo().param_bytes())
        self.assertEqual(int.from_bytes(raw[4:6], "little"), export.FORMAT_VERSION)
        self.assertEqual(int.from_bytes(raw[8:12], "little"), PERSPECTIVE_768_DIM)
        self.assertEqual(int.from_bytes(raw[12:16], "little"), 16)  # hidden width
        self.assertEqual(int.from_bytes(raw[20:22], "little"), 255)  # qa
        self.assertEqual(int.from_bytes(raw[22:24], "little"), 64)  # qb
        # Reserved bytes are all zero.
        self.assertEqual(raw[40:64], bytes(24))

    def test_reader_rejects_corruption_like_the_engine_loader(self):
        raw = bytearray(self._demo().to_bytes())
        with self.assertRaises(ExportError):
            QuantizedNetwork.from_bytes(bytes(raw[:-1]))  # truncated blob
        with self.assertRaises(ExportError):
            QuantizedNetwork.from_bytes(bytes(raw) + b"\x00")  # trailing byte
        bad_magic = bytearray(raw)
        bad_magic[0] = ord("X")
        with self.assertRaises(ExportError):
            QuantizedNetwork.from_bytes(bytes(bad_magic))
        corrupt = bytearray(raw)
        corrupt[export.HEADER_LEN] ^= 0x01  # flip a weight bit -> hash mismatch
        with self.assertRaises(ExportError):
            QuantizedNetwork.from_bytes(bytes(corrupt))


class IntegerInferenceTest(unittest.TestCase):
    def test_constant_accumulator_matches_a_hand_computation(self):
        # Zero feature weights leave every accumulator entry at the bias, so the
        # integer forward pass is a closed form we can compute by hand -- the same
        # check the Rust inference tests make.
        h, qa, qb, scale = 16, 255, 64, 400
        entry, w_out_value = 100, 3  # 0 < 100 < qa, so no clipping
        net = QuantizedNetwork(
            hidden=h,
            qa=qa,
            qb=qb,
            scale=scale,
            w_ft=np.zeros(PERSPECTIVE_768_DIM * h, dtype=np.int16),
            b_ft=np.full(h, entry, dtype=np.int16),
            w_out=np.full(2 * h, w_out_value, dtype=np.int16),
            b_out=np.zeros(1, dtype=np.int32),
        )
        # s = 2H * entry * w_out; eval = round_half_away(s * scale / (qa*qb)).
        s = 2 * h * entry * w_out_value
        num, den = s * scale, qa * qb
        expected = (num + den // 2) // den
        got = integer_eval_cp(net, np.array([0, 5], dtype=np.int64), np.array([9], dtype=np.int64))
        self.assertEqual(got, expected)


class ReproductionTest(unittest.TestCase):
    def test_exported_network_reproduces_the_trained_model(self):
        # Train a small quantization-aware model on a synthetic but learnable
        # signal (an advanced white pawn wins), quantize it, and check the integer
        # forward pass reproduces the model's own centipawn evaluation across every
        # fixture position to within the dequantizing divide's rounding.
        rng = np.random.default_rng(0)
        records = []
        for _ in range(1200):
            pawn = 8 + int(rng.integers(0, 48))
            win = pawn >= 32
            records.append(
                encode_record(
                    {4: WHITE_KING, 60: BLACK_KING, pawn: WHITE_PAWN},
                    score=600 if win else -600,
                    wdl=2 if win else 0,
                )
            )

        class _InMemory(data.PackedData):
            def __init__(self, recs):
                self.records = np.stack(recs)

        dataset = _InMemory(records)
        model, _ = train.train(
            dataset,
            NnueConfig(hidden=32),
            epochs=10,
            batch_size=256,
            lr=1e-2,
            lam=0.5,
            val_fraction=0.2,
            seed=0,
            log=lambda *_: None,
        )
        model.eval()
        net = quantize(model)

        batch = dataset.batch(np.arange(len(dataset)))
        with torch.no_grad():
            fout = model(
                torch.from_numpy(batch.stm_indices),
                torch.from_numpy(batch.offsets),
                torch.from_numpy(batch.nstm_indices),
                torch.from_numpy(batch.offsets),
            ).numpy()
        float_cp = model.config.scale * fout

        offsets = batch.offsets
        total = len(batch)
        worst = 0.0
        for k in range(total):
            start = offsets[k]
            end = offsets[k + 1] if k + 1 < total else batch.stm_indices.shape[0]
            got = integer_eval_cp(net, batch.stm_indices[start:end], batch.nstm_indices[start:end])
            worst = max(worst, abs(got - float_cp[k]))
        self.assertLessEqual(worst, _REPRODUCTION_TOLERANCE_CP, f"max reproduction error {worst:.3f}cp")


# Piece code -> FEN letter, the inverse of the exporter's FEN parse table, so a
# placement can be rendered back to the FEN the parser reads.
_CODE_TO_FEN = {code: letter for letter, code in export._FEN_PIECE_CODES.items()}


def _pieces_to_fen(pieces: dict[int, int], black_to_move: bool = False) -> str:
    """Render a ``{square: code}`` placement (square A1 = 0) as a full six-field
    FEN, so a position built for the packed-record path can be handed to
    :func:`features_from_fen` as the same board."""
    ranks = []
    for rank in range(7, -1, -1):  # FEN lists rank 8 first
        row, empty = "", 0
        for file in range(8):
            code = pieces.get(rank * 8 + file)
            if code is None:
                empty += 1
                continue
            if empty:
                row += str(empty)
                empty = 0
            row += _CODE_TO_FEN[code]
        if empty:
            row += str(empty)
        ranks.append(row or "8")
    return "/".join(ranks) + (" b" if black_to_move else " w") + " - - 0 1"


def _parse_vectors_file(path: Path) -> list[tuple[str, str, int]]:
    """Parse a committed ``.vectors`` file into ``(category, FEN, cp)`` triples the
    same way the engine's differential test does: skip ``#`` comments, split each
    line on tabs."""
    triples = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        category, fen, cp = line.split("\t")
        triples.append((category, fen, int(cp)))
    return triples


class FeaturesFromFenTest(unittest.TestCase):
    """``features_from_fen`` is a second, FEN-based derivation of the feature
    indices; it must agree with the packed-record decode path (:func:`data.decode`),
    the way the engine's own board derivation must agree with both."""

    def _placements(self):
        # A middlegame-ish scatter, an endgame, and a queen on a central square,
        # each also tried with black to move so the perspective swap is exercised.
        return [
            {4: WHITE_KING, 60: BLACK_KING, 12: WHITE_PAWN, 51: BLACK_PAWN, 0: WHITE_ROOK, 63: BLACK_ROOK},
            {6: WHITE_KING, 58: BLACK_KING, 8: WHITE_PAWN},
            {4: WHITE_KING, 60: BLACK_KING, 27: WHITE_QUEEN, 35: BLACK_PAWN, 24: BLACK_ROOK},
        ]

    def test_agrees_with_the_packed_record_decode(self):
        for pieces in self._placements():
            for black_to_move in (False, True):
                record = encode_record(pieces, black_to_move=black_to_move)
                batch = data.decode(record[None, :])  # one record: all indices are its own
                fen = _pieces_to_fen(pieces, black_to_move)
                fen_stm, fen_nstm = features_from_fen(fen)
                # Compare as multisets: both paths list one index per piece, but the
                # square order they emit need not match.
                self.assertEqual(
                    sorted(fen_stm.tolist()), sorted(batch.stm_indices.tolist()), fen
                )
                self.assertEqual(
                    sorted(fen_nstm.tolist()), sorted(batch.nstm_indices.tolist()), fen
                )

    def test_indices_stay_in_range(self):
        for cat, fen in GOLDEN_POSITIONS:
            stm, nstm = features_from_fen(fen)
            for arr in (stm, nstm):
                self.assertTrue(arr.size)
                self.assertGreaterEqual(int(arr.min()), 0)
                self.assertLess(int(arr.max()), PERSPECTIVE_768_DIM)

    def test_rejects_malformed_placements(self):
        for bad in (
            "8/8/8/8/8/8/8/4X3 w - - 0 1",  # unknown piece letter
            "8/8/8 w - - 0 1",  # too few ranks
            "7/8/8/8/8/8/8/8 w - - 0 1",  # a rank that does not fill eight files
            "9/8/8/8/8/8/8/8 w - - 0 1",  # a rank that overflows eight files
            "4k2pp/8/8/8/8/8/8/4K3 w - - 0 1",  # a piece past the eighth file
        ):
            with self.assertRaises(ExportError, msg=bad):
                features_from_fen(bad)


class GoldenVectorTest(unittest.TestCase):
    """The golden-vector emission: the curated set spans the required position
    kinds, the near-overflow set genuinely stresses the wide integer regime, and
    the committed fixture matches what the exporter emits now."""

    def test_positions_span_every_category(self):
        self.assertEqual(
            {category for category, _ in GOLDEN_POSITIONS},
            {"tactical", "endgame", "king-safety", "near-overflow"},
        )

    def test_golden_network_is_bounded_and_uses_contract_dimensions(self):
        # Constructing it runs the exporter's accumulator-overflow guard, so this
        # asserts no legal position can overflow the i16 accumulator.
        net = export._golden_network()
        self.assertEqual((net.hidden, net.qa, net.qb, net.scale), (16, 255, 64, 400))
        export._assert_accumulator_fits_i16(net)

    def test_near_overflow_positions_drive_the_widest_accumulators(self):
        # The point of the near-overflow category: those positions push an
        # accumulator entry to a large fraction of the i16 it is held in (here past
        # 0.7 of i16::MAX), further than any other category reaches, while still
        # fitting -- exactly the regime a too-narrow intermediate type would break.
        net = export._golden_network()
        columns = net.w_ft.reshape(PERSPECTIVE_768_DIM, net.hidden).astype(np.int64)
        bias = net.b_ft.astype(np.int64)

        def peak_accumulator(fen: str) -> int:
            stm, nstm = features_from_fen(fen)
            own = bias + columns[stm].sum(axis=0)
            enemy = bias + columns[nstm].sum(axis=0)
            return int(max(np.abs(own).max(), np.abs(enemy).max()))

        near = [peak_accumulator(fen) for cat, fen in GOLDEN_POSITIONS if cat == "near-overflow"]
        others = [peak_accumulator(fen) for cat, fen in GOLDEN_POSITIONS if cat != "near-overflow"]
        i16_max = 32767
        self.assertTrue(near, "there are near-overflow positions")
        for peak in near:
            self.assertGreater(peak, 0.7 * i16_max)
            self.assertLessEqual(peak, i16_max)  # fits: the guard forbids an overflow
        self.assertGreaterEqual(min(near), max(others))

    def test_committed_fixture_matches_current_export(self):
        # The committed expected scores are the exporter's integer forward pass over
        # the committed network; if either drifts, the engine's differential test
        # would compare against stale values, so this fails first and asks for a
        # regeneration. Mirrors `--emit-golden`.
        net = export._golden_network()
        committed_net = (_GOLDEN_VECTORS_PATH.parent / "golden_v1.sbnn").read_bytes()
        self.assertEqual(net.to_bytes(), committed_net, "golden_v1.sbnn is stale; re-emit")
        self.assertEqual(
            golden_vectors(net),
            _parse_vectors_file(_GOLDEN_VECTORS_PATH),
            "golden_v1.vectors is stale; re-emit",
        )

    def test_golden_network_round_trips_through_the_reader(self):
        net = export._golden_network()
        reloaded = QuantizedNetwork.from_bytes(net.to_bytes())
        self.assertEqual((reloaded.hidden, reloaded.qa, reloaded.qb, reloaded.scale), (16, 255, 64, 400))
        np.testing.assert_array_equal(reloaded.w_ft, net.w_ft)
        np.testing.assert_array_equal(reloaded.b_ft, net.b_ft)
        np.testing.assert_array_equal(reloaded.w_out, net.w_out)
        np.testing.assert_array_equal(reloaded.b_out, net.b_out)

    def test_write_golden_fixture_emits_a_loadable_pair(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            net_path = Path(tmp) / "golden.sbnn"
            vectors_path = Path(tmp) / "golden.vectors"
            vectors = export.write_golden_fixture(net_path, vectors_path)
            # The written network round-trips and the written vectors reparse to the
            # returned triples.
            QuantizedNetwork.from_bytes(net_path.read_bytes())
            self.assertEqual(vectors, _parse_vectors_file(vectors_path))
            self.assertEqual([category for category, _, _ in vectors], [c for c, _ in GOLDEN_POSITIONS])


if __name__ == "__main__":
    unittest.main()
