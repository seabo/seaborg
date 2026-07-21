"""Quantize a trained NNUE model and write the versioned ``SBNN`` network file.

This is the export half of the pipeline: it turns the float checkpoint the
trainer produces into the integer network the engine loads and runs. The byte
layout, the quantization scales, and the integer types are all fixed by
``docs/nnue-design-contract.md`` and mirror ``engine/src/nnue/format.rs`` exactly
-- the file is the sole contract carrying weights across the language boundary,
so a byte that disagrees is a file the engine rejects.

Quantization (round half to even, the NumPy/PyTorch ``.round()`` default):

    W_ft = round(w · QA)   i16      feature-transformer weights, feature-major
    b_ft = round(b · QA)   i16      feature-transformer bias
    W_out = round(w · QB)  i16      output weights, own block then enemy block
    b_out = round(b · QA · QB)  i32 output bias

Because the trainer is quantization-aware (:mod:`model`), the float model already
computes on these rounded values, so the exported integer network reproduces the
model's behaviour rather than a nearby function it never trained on. This module
verifies that claim two ways: it refuses to write a network whose accumulator
could overflow the i16 the engine holds it in, and :func:`integer_eval_cp`
reproduces the contract's integer forward pass (the same arithmetic as
``engine::nnue::forward``) so a caller can measure the export against the float
model.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

import numpy as np

from model import (
    MAX_ACTIVE_FEATURES,
    PERSPECTIVE_768_DIM,
    PERSPECTIVE_768_ID,
    NnueConfig,
    NnueModel,
)

# SBNN header constants, matching engine/src/nnue/format.rs. A file that disagrees
# with any of these is one the engine loader refuses.
MAGIC = b"SBNN"
HEADER_LEN = 64
FORMAT_VERSION = 1
ACTIVATION_CRELU = 0
OUTPUT_DIM = 1

# Header field byte offsets (little-endian throughout).
_OFF_MAGIC = 0
_OFF_FORMAT_VERSION = 4
_OFF_FEATURE_SET_ID = 6
_OFF_INPUT_DIM = 8
_OFF_HIDDEN_WIDTH = 12
_OFF_OUTPUT_DIM = 16
_OFF_ACTIVATION_ID = 18
_OFF_QA = 20
_OFF_QB = 22
_OFF_SCALE = 24
_OFF_PARAM_BYTES = 28
_OFF_PARAM_HASH = 32
_OFF_RESERVED = 40

_HIDDEN_WIDTH_MULTIPLE = 16
_I16_MIN, _I16_MAX = -32768, 32767
_I32_MIN, _I32_MAX = -(2**31), 2**31 - 1
_EVAL_CP_MIN, _EVAL_CP_MAX = -10_000, 10_000


class ExportError(ValueError):
    """A model could not be exported: a weight overflowed its integer type, or the
    accumulator bound the engine relies on would be violated."""


def _fnv1a_64(blob: bytes) -> int:
    """64-bit FNV-1a hash of the parameter blob, matching the Rust loader's guard
    against corruption and truncation."""
    offset_basis = 0xCBF29CE484222325
    prime = 0x100000001B3
    mask = 0xFFFFFFFFFFFFFFFF
    h = offset_basis
    for byte in blob:
        h ^= byte
        h = (h * prime) & mask
    return h


def _round_half_even(values: np.ndarray, scale: float) -> np.ndarray:
    """Scale then round to the nearest integer, halves to even -- the rounding the
    contract fixes for every weight the exporter writes."""
    return np.rint(values.astype(np.float64) * scale)


def _checked_cast(values: np.ndarray, lo: int, hi: int, block: str, dtype) -> np.ndarray:
    """Cast rounded values to an integer ``dtype``, refusing any that fall outside
    the type's range rather than wrapping them into a different weight."""
    if values.size and (values.min() < lo or values.max() > hi):
        raise ExportError(
            f"quantized `{block}` weight {values.min():.0f}..{values.max():.0f} "
            f"leaves the [{lo}, {hi}] range of its integer type"
        )
    return values.astype(dtype)


@dataclass(frozen=True)
class QuantizedNetwork:
    """A quantized network in the engine's on-disk integer types: the
    parameterizable dimensions plus the four weight blocks. Construct one with
    :func:`quantize` from a trained model, or :meth:`from_bytes` from a file."""

    hidden: int
    qa: int
    qb: int
    scale: int
    w_ft: np.ndarray  # int16, INPUT_DIM * H, feature-major (feature f at f*H)
    b_ft: np.ndarray  # int16, H
    w_out: np.ndarray  # int16, 2H, own block then enemy block
    b_out: np.ndarray  # int32, OUTPUT_DIM

    def param_bytes(self) -> int:
        return 2 * self.w_ft.size + 2 * self.b_ft.size + 2 * self.w_out.size + 4 * self.b_out.size

    def _blob(self) -> bytes:
        """The parameter blob in the fixed on-disk order, little-endian."""
        return b"".join(
            (
                self.w_ft.astype("<i2").tobytes(),
                self.b_ft.astype("<i2").tobytes(),
                self.w_out.astype("<i2").tobytes(),
                self.b_out.astype("<i4").tobytes(),
            )
        )

    def to_bytes(self) -> bytes:
        """Serialise to the 64-byte header followed by the parameter blob."""
        blob = self._blob()
        header = bytearray(HEADER_LEN)
        header[_OFF_MAGIC : _OFF_MAGIC + 4] = MAGIC
        header[_OFF_FORMAT_VERSION : _OFF_FORMAT_VERSION + 2] = FORMAT_VERSION.to_bytes(2, "little")
        header[_OFF_FEATURE_SET_ID : _OFF_FEATURE_SET_ID + 2] = PERSPECTIVE_768_ID.to_bytes(
            2, "little"
        )
        header[_OFF_INPUT_DIM : _OFF_INPUT_DIM + 4] = PERSPECTIVE_768_DIM.to_bytes(4, "little")
        header[_OFF_HIDDEN_WIDTH : _OFF_HIDDEN_WIDTH + 4] = int(self.hidden).to_bytes(4, "little")
        header[_OFF_OUTPUT_DIM : _OFF_OUTPUT_DIM + 2] = OUTPUT_DIM.to_bytes(2, "little")
        header[_OFF_ACTIVATION_ID : _OFF_ACTIVATION_ID + 2] = ACTIVATION_CRELU.to_bytes(2, "little")
        header[_OFF_QA : _OFF_QA + 2] = int(self.qa).to_bytes(2, "little")
        header[_OFF_QB : _OFF_QB + 2] = int(self.qb).to_bytes(2, "little")
        header[_OFF_SCALE : _OFF_SCALE + 4] = int(self.scale).to_bytes(4, "little", signed=True)
        header[_OFF_PARAM_BYTES : _OFF_PARAM_BYTES + 4] = len(blob).to_bytes(4, "little")
        header[_OFF_PARAM_HASH : _OFF_PARAM_HASH + 8] = _fnv1a_64(blob).to_bytes(8, "little")
        # Reserved bytes stay zero, matching the writer the engine validates against.
        return bytes(header) + blob

    @classmethod
    def from_bytes(cls, data: bytes) -> "QuantizedNetwork":
        """Parse and validate a file the same way the engine loader does. Written
        independently of :meth:`to_bytes` so a round-trip test exercises the byte
        layout from both directions; every rejection here mirrors a distinct
        ``LoadError`` in ``engine/src/nnue/format.rs``."""
        if len(data) < HEADER_LEN:
            raise ExportError("shorter than the 64-byte header")
        header = data[:HEADER_LEN]

        def u16(off: int) -> int:
            return int.from_bytes(header[off : off + 2], "little")

        def u32(off: int) -> int:
            return int.from_bytes(header[off : off + 4], "little")

        if header[_OFF_MAGIC : _OFF_MAGIC + 4] != MAGIC:
            raise ExportError("bad magic")
        if u16(_OFF_FORMAT_VERSION) != FORMAT_VERSION:
            raise ExportError(f"unsupported version {u16(_OFF_FORMAT_VERSION)}")
        if u16(_OFF_FEATURE_SET_ID) != PERSPECTIVE_768_ID:
            raise ExportError(f"unsupported feature set {u16(_OFF_FEATURE_SET_ID)}")
        if u16(_OFF_ACTIVATION_ID) != ACTIVATION_CRELU:
            raise ExportError(f"unsupported activation {u16(_OFF_ACTIVATION_ID)}")
        input_dim = u32(_OFF_INPUT_DIM)
        if input_dim != PERSPECTIVE_768_DIM:
            raise ExportError(f"input dim {input_dim} inconsistent with feature set")
        hidden = u32(_OFF_HIDDEN_WIDTH)
        if hidden == 0 or hidden % _HIDDEN_WIDTH_MULTIPLE != 0:
            raise ExportError(f"hidden width {hidden} is not a positive multiple of 16")
        if u16(_OFF_OUTPUT_DIM) != OUTPUT_DIM:
            raise ExportError(f"output dim {u16(_OFF_OUTPUT_DIM)} unsupported")
        qa, qb = u16(_OFF_QA), u16(_OFF_QB)
        scale = int.from_bytes(header[_OFF_SCALE : _OFF_SCALE + 4], "little", signed=True)
        if qa <= 0 or qb <= 0 or scale <= 0:
            raise ExportError("qa, qb, and scale must be positive")
        if any(header[_OFF_RESERVED:HEADER_LEN]):
            raise ExportError("reserved bytes are non-zero")

        expected = 2 * input_dim * hidden + 2 * hidden + 2 * (2 * hidden) + 4 * OUTPUT_DIM
        if u32(_OFF_PARAM_BYTES) != expected:
            raise ExportError("param_bytes disagrees with the dimensions")
        blob = data[HEADER_LEN:]
        if len(blob) < expected:
            raise ExportError("truncated parameter blob")
        if len(blob) > expected:
            raise ExportError("trailing bytes beyond the parameter blob")
        declared_hash = int.from_bytes(header[_OFF_PARAM_HASH : _OFF_PARAM_HASH + 8], "little")
        if declared_hash != _fnv1a_64(blob):
            raise ExportError("parameter blob hash mismatch")

        pos = 0

        def take(count: int, dtype: str) -> np.ndarray:
            nonlocal pos
            width = np.dtype(dtype).itemsize
            arr = np.frombuffer(blob, dtype=dtype, count=count, offset=pos).copy()
            pos += count * width
            return arr

        return cls(
            hidden=hidden,
            qa=qa,
            qb=qb,
            scale=scale,
            w_ft=take(input_dim * hidden, "<i2"),
            b_ft=take(hidden, "<i2"),
            w_out=take(2 * hidden, "<i2"),
            b_out=take(OUTPUT_DIM, "<i4"),
        )


def _assert_accumulator_fits_i16(net: QuantizedNetwork) -> None:
    """Refuse a network whose i16 accumulator could overflow for a legal position.

    The engine holds each perspective's accumulator in i16 and treats an overflow
    as a defect, not a wrap. For a hidden unit the accumulator is ``b_ft`` plus at
    most :data:`MAX_ACTIVE_FEATURES` weight columns (one per piece), so the tightest
    reachable magnitude is ``|b_ft| + Σ`` of that unit's 32 largest ``|W_ft|``. If
    every unit stays inside i16, no legal position can overflow."""
    columns = net.w_ft.reshape(PERSPECTIVE_768_DIM, net.hidden).astype(np.int64)
    largest = np.sort(np.abs(columns), axis=0)[-MAX_ACTIVE_FEATURES:]
    worst = np.abs(net.b_ft.astype(np.int64)) + largest.sum(axis=0)
    peak = int(worst.max()) if worst.size else 0
    if peak > _I16_MAX:
        raise ExportError(
            f"accumulator could reach {peak}, past i16::MAX ({_I16_MAX}); "
            "the feature-transformer weights are not bounded for i16"
        )


def quantize(model: NnueModel) -> QuantizedNetwork:
    """Quantize a trained model to the engine's integer network, checking that no
    weight overflows its type and that the accumulator stays inside i16."""
    config = model.config
    if config.activation_id != ACTIVATION_CRELU:
        raise ExportError(
            f"activation {config.activation!r} has no v1 integer inference; export needs crelu"
        )
    state = model.state_dict()
    w_ft = state["feature_transformer.weight"].detach().cpu().numpy()  # [768, H]
    b_ft = state["ft_bias"].detach().cpu().numpy()  # [H]
    w_out = state["output.weight"].detach().cpu().numpy()  # [1, 2H]
    b_out = state["output.bias"].detach().cpu().numpy()  # [1]

    net = QuantizedNetwork(
        hidden=config.hidden,
        qa=config.qa,
        qb=config.qb,
        scale=config.scale,
        # Row-major flatten of [768, H] is the feature-major f*H + i order on disk.
        w_ft=_checked_cast(
            _round_half_even(w_ft, config.qa).reshape(-1), _I16_MIN, _I16_MAX, "w_ft", np.int16
        ),
        b_ft=_checked_cast(
            _round_half_even(b_ft, config.qa), _I16_MIN, _I16_MAX, "b_ft", np.int16
        ),
        w_out=_checked_cast(
            _round_half_even(w_out.reshape(-1), config.qb), _I16_MIN, _I16_MAX, "w_out", np.int16
        ),
        b_out=_checked_cast(
            _round_half_even(b_out, config.qa * config.qb), _I32_MIN, _I32_MAX, "b_out", np.int32
        ),
    )
    _assert_accumulator_fits_i16(net)
    return net


def integer_eval_cp(
    net: QuantizedNetwork, stm_features: np.ndarray, nstm_features: np.ndarray
) -> int:
    """The contract's integer forward pass for one position, in centipawns from the
    side to move. This is the same arithmetic as ``engine::nnue::forward``: an i16
    accumulator per perspective, activations clipped to ``[0, QA]``, an i32 output
    sum, then a rounded (half away from zero) dequantizing divide by ``QA·QB``.

    ``stm_features`` and ``nstm_features`` are the active feature indices for the
    side-to-move and other perspectives (what :func:`data.decode` produces)."""
    h = net.hidden
    columns = net.w_ft.reshape(PERSPECTIVE_768_DIM, h).astype(np.int64)
    bias = net.b_ft.astype(np.int64)

    own = bias + columns[np.asarray(stm_features, dtype=np.int64)].sum(axis=0)
    enemy = bias + columns[np.asarray(nstm_features, dtype=np.int64)].sum(axis=0)
    own = np.clip(own, 0, net.qa)
    enemy = np.clip(enemy, 0, net.qa)

    w_out = net.w_out.astype(np.int64)
    s = int(net.b_out[0])
    s += int(own @ w_out[:h])
    s += int(enemy @ w_out[h:])

    num = s * net.scale
    den = net.qa * net.qb
    half = den // 2
    cp = (num + half) // den if num >= 0 else -((-num + half) // den)
    return int(np.clip(cp, _EVAL_CP_MIN, _EVAL_CP_MAX))


def write_network(path, model: NnueModel) -> QuantizedNetwork:
    """Quantize ``model`` and write the SBNN file at ``path``; return the quantized
    network so a caller can inspect or reproduce it."""
    net = quantize(model)
    Path(path).write_bytes(net.to_bytes())
    return net


def _load_checkpoint_model(path) -> NnueModel:
    """Rebuild the trained model from a checkpoint written by
    :func:`train.save_checkpoint`. It is loaded quantization-aware so a reproduction
    self-check compares against the behaviour training actually optimised."""
    import torch

    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    config = NnueConfig(**checkpoint["config"])
    model = NnueModel(config, quantization_aware=True)
    model.load_state_dict(checkpoint["state_dict"])
    model.eval()
    return model


def _demo_network(hidden: int = 16) -> QuantizedNetwork:
    """A deterministic, patterned network used as a cross-language fixture: the
    Python exporter writes it and the engine's integration test reads it, so the
    two agree on the byte layout. The pattern varies every weight so a dropped or
    reordered block would change a value rather than compare equal by coincidence."""
    features = PERSPECTIVE_768_DIM
    f = np.arange(features)[:, None]
    i = np.arange(hidden)[None, :]
    w_ft = (((f * 31 + i * 7) % 41) - 20).reshape(-1).astype(np.int16)
    b_ft = ((np.arange(hidden) % 7) - 3).astype(np.int16)
    j = np.arange(2 * hidden)
    w_out = (((j * 13) % 49) - 24).astype(np.int16)
    b_out = np.array([0], dtype=np.int32)
    return QuantizedNetwork(
        hidden=hidden, qa=255, qb=64, scale=400, w_ft=w_ft, b_ft=b_ft, w_out=w_out, b_out=b_out
    )


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Quantize and export an NNUE network file.")
    parser.add_argument("--checkpoint", type=Path, help="fp32 checkpoint from train.py")
    parser.add_argument("--out", type=Path, help="write the SBNN network file here")
    parser.add_argument(
        "--emit-fixture",
        type=Path,
        help="write the deterministic cross-language test fixture and exit",
    )
    args = parser.parse_args(argv)

    if args.emit_fixture is not None:
        args.emit_fixture.write_bytes(_demo_network().to_bytes())
        print(f"wrote fixture to {args.emit_fixture}")
        return 0

    if args.checkpoint is None or args.out is None:
        parser.error("--checkpoint and --out are required unless --emit-fixture is given")

    model = _load_checkpoint_model(args.checkpoint)
    net = write_network(args.out, model)
    print(
        f"wrote {args.out}: H={net.hidden} qa={net.qa} qb={net.qb} scale={net.scale}, "
        f"{net.param_bytes()} parameter bytes"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
