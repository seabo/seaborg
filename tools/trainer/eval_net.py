"""Standalone value-fidelity evaluation of an exported network on a corpus.

Loads a version-1 SBNN, reconstructs the float model it exported from, and reports
the same value-fidelity metrics ``train.py`` prints (see :mod:`metrics`) — so any
shipped or candidate network can be assessed on a corpus's held-out validation
split without retraining. Because the reconstruction is the exact inverse of the
quantized export, the reported loss reproduces the number training recorded.

Bucketed (version-2) networks are evaluated by ``train.py`` during their own
training run; this standalone path covers the single-output v1 networks.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import torch

from data import BatchLoader, PackedData, default_num_workers
from export import QuantizedNetwork
from metrics import format_value_fidelity
from model import ACTIVATION_IDS, NnueConfig, NnueModel
from split import by_shard_split, load_manifest
from train import value_fidelity_eval

_ID2ACT = {v: k for k, v in ACTIVATION_IDS.items()}


def load_sbnn_model(path) -> tuple[NnueModel, int]:
    """Rebuild the float :class:`NnueModel` a version-1 SBNN exported from, by
    dequantizing its integer weights. Returns the model and the network's scale."""
    try:
        q = QuantizedNetwork.from_bytes(Path(path).read_bytes())
    except Exception as exc:  # noqa: BLE001 - surfaced as a clean CLI error
        raise SystemExit(
            f"{path}: could not read as a version-1 SBNN ({exc}); the standalone "
            "value-fidelity path supports single-output v1 networks."
        )
    h = q.hidden
    activation = q.activation if isinstance(q.activation, str) else _ID2ACT[int(q.activation)]
    config = NnueConfig(hidden=h, activation=activation, qa=q.qa, qb=q.qb, scale=q.scale)
    model = NnueModel(config, quantization_aware=True)
    state = model.state_dict()
    state["feature_transformer.weight"] = torch.tensor(
        q.w_ft.reshape(768, h).astype(np.float32) / q.qa
    )
    state["ft_bias"] = torch.tensor(q.b_ft.astype(np.float32) / q.qa)
    state["output.weight"] = torch.tensor(q.w_out.reshape(1, 2 * h).astype(np.float32) / q.qb)
    state["output.bias"] = torch.tensor(q.b_out.astype(np.float32) / (q.qa * q.qb))
    model.load_state_dict(state)
    return model, q.scale


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--net", type=Path, required=True, help="exported .sbnn network")
    parser.add_argument("--corpus", type=Path, required=True, help="packed corpus (.bin)")
    parser.add_argument(
        "--manifest", type=Path, default=None, help="provenance manifest; defaults beside the corpus"
    )
    parser.add_argument("--val-fraction", type=float, default=0.1)
    parser.add_argument("--split-seed", type=int, default=0)
    parser.add_argument("--batch-size", type=int, default=8192)
    parser.add_argument("--num-workers", type=int, default=default_num_workers())
    parser.add_argument("--device", default="cpu")
    args = parser.parse_args(argv)

    data = PackedData(args.corpus)
    manifest = args.manifest if args.manifest is not None else args.corpus.parent
    split = by_shard_split(load_manifest(manifest), val_fraction=args.val_fraction, seed=args.split_seed)
    model, scale = load_sbnn_model(args.net)
    model.to(args.device)
    print(
        f"{args.net.name}: {len(split.val_idx):,} val positions "
        f"({len(split.val_shards)} of {len(split.val_shards) + len(split.train_shards)} "
        f"shards held out), scale={scale}"
    )
    with BatchLoader(data, args.num_workers) as loader:
        vf = value_fidelity_eval(model, loader, split.val_idx, args.batch_size, args.device, scale)
    print(format_value_fidelity(vf))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
