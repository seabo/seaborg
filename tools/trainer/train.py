"""Train the float NNUE network on packed self-play samples.

This delivers the first half of the training pipeline: consume generated data
and produce an fp32 checkpoint, reporting training and validation loss. The
quantized export and the strength-preserving numeric guarantees are separate,
later tasks; this checkpoint is float weights only.

The training target is the contract's blended, win-probability-space target::

    r            = wdl / 2                       game outcome for the side to move
    score_target = sigmoid(search_cp / SCALE)    search score as a win probability
    y            = lambda * r + (1 - lambda) * score_target
    p            = sigmoid(fout)                  model prediction
    loss         = (p - y)^2                      MSE in win-probability space

The same SCALE converts centipawns to win probability here and the network's
output to centipawns at inference, so the value the network learns to emit and
the value search consumes are the same quantity. ``lambda`` is the weight on the
game outcome (0 trusts search entirely, 1 trusts the result entirely).
"""

from __future__ import annotations

import argparse
import time
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np
import torch

from data import PackedData, iter_batches
from model import NnueConfig, NnueModel


def _sigmoid(x: np.ndarray) -> np.ndarray:
    return 1.0 / (1.0 + np.exp(-x))


def targets(score: np.ndarray, wdl: np.ndarray, scale: float, lam: float) -> np.ndarray:
    """The blended win-probability target ``y`` for a batch. Pure NumPy: the
    target carries no gradient."""
    r = wdl.astype(np.float64) / 2.0
    score_target = _sigmoid(score.astype(np.float64) / scale)
    return lam * r + (1.0 - lam) * score_target


def _to_device(batch, device):
    """Move a decoded batch's sparse tensors onto the training device."""
    stm_idx = torch.from_numpy(batch.stm_indices).to(device)
    nstm_idx = torch.from_numpy(batch.nstm_indices).to(device)
    offsets = torch.from_numpy(batch.offsets).to(device)
    return stm_idx, nstm_idx, offsets


def _loss_on(model, batch, device, scale, lam) -> torch.Tensor:
    stm_idx, nstm_idx, offsets = _to_device(batch, device)
    y = torch.from_numpy(targets(batch.score, batch.wdl, scale, lam)).to(
        device=device, dtype=torch.float32
    )
    fout = model(stm_idx, offsets, nstm_idx, offsets)
    p = torch.sigmoid(fout)
    return torch.mean((p - y) ** 2)


@dataclass
class EpochReport:
    epoch: int
    train_loss: float
    val_loss: float


def _evaluate(model, data, indices, batch_size, device, scale, lam) -> float:
    """Mean validation loss, weighted by batch size so a short final batch does
    not skew the average."""
    model.eval()
    total = 0.0
    seen = 0
    with torch.no_grad():
        for batch in iter_batches(data, indices, batch_size):
            n = len(batch)
            total += _loss_on(model, batch, device, scale, lam).item() * n
            seen += n
    return total / max(seen, 1)


def train(
    data: PackedData,
    config: NnueConfig,
    *,
    epochs: int,
    batch_size: int,
    lr: float,
    lam: float,
    val_fraction: float,
    seed: int,
    device: str = "cpu",
    log=print,
) -> tuple[NnueModel, list[EpochReport]]:
    """Train a fresh model and return it with its per-epoch loss history."""
    torch.manual_seed(seed)
    rng = np.random.default_rng(seed)

    order = rng.permutation(len(data))
    val_size = int(len(order) * val_fraction)
    val_idx = order[:val_size]
    train_idx = order[val_size:]
    if len(train_idx) == 0:
        raise ValueError("no training samples remain after the validation split")

    model = NnueModel(config).to(device)
    optimizer = torch.optim.Adam(model.parameters(), lr=lr)

    history: list[EpochReport] = []
    for epoch in range(1, epochs + 1):
        model.train()
        epoch_order = train_idx[rng.permutation(len(train_idx))]
        total = 0.0
        seen = 0
        for batch in iter_batches(data, epoch_order, batch_size):
            loss = _loss_on(model, batch, device, config.scale, lam)
            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
            n = len(batch)
            total += loss.item() * n
            seen += n
        train_loss = total / max(seen, 1)
        val_loss = (
            _evaluate(model, data, val_idx, batch_size, device, config.scale, lam)
            if len(val_idx) > 0
            else float("nan")
        )
        history.append(EpochReport(epoch, train_loss, val_loss))
        log(f"epoch {epoch:3d}  train_loss {train_loss:.6f}  val_loss {val_loss:.6f}")

    return model, history


def save_checkpoint(path, model: NnueModel, history: list[EpochReport]) -> None:
    """Write the fp32 checkpoint: architecture config plus float weights. The
    config is enough to rebuild the exact network; the quantized export reads
    this file."""
    torch.save(
        {
            "format": "seaborg-nnue-fp32",
            "config": asdict(model.config),
            "state_dict": model.state_dict(),
            "history": [asdict(r) for r in history],
        },
        path,
    )


def benchmark_dataloader(data: PackedData, batch_size: int, seconds: float, log=print) -> float:
    """Measure decode throughput (samples/sec) over shuffled batches, so the
    figure reflects the random-access pattern training uses. Returns the rate."""
    rng = np.random.default_rng(0)
    order = rng.permutation(len(data))
    processed = 0
    # A short warm-up faults the memmap pages in so the timed run measures decode
    # work, not first-touch page faults.
    for batch in iter_batches(data, order[: min(len(order), 4 * batch_size)], batch_size):
        processed += len(batch)
    processed = 0
    start = time.perf_counter()
    elapsed = 0.0
    while elapsed < seconds:
        for batch in iter_batches(data, order, batch_size):
            processed += len(batch)
        elapsed = time.perf_counter() - start
    rate = processed / elapsed
    log(f"dataloader throughput: {rate:,.0f} samples/sec (batch_size={batch_size})")
    return rate


def _build_config(args) -> NnueConfig:
    return NnueConfig(hidden=args.hidden, activation=args.activation, scale=args.scale)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Train the float NNUE network.")
    parser.add_argument("--data", type=Path, required=True, help="packed sample file")
    parser.add_argument("--epochs", type=int, default=30)
    parser.add_argument("--batch-size", type=int, default=8192)
    parser.add_argument("--lr", type=float, default=1e-2)
    parser.add_argument("--hidden", type=int, default=256, help="hidden width H (multiple of 16)")
    parser.add_argument("--activation", choices=["crelu", "screlu"], default="crelu")
    parser.add_argument("--scale", type=int, default=400)
    parser.add_argument(
        "--lambda", dest="lam", type=float, default=0.3, help="weight on the game outcome"
    )
    parser.add_argument("--val-fraction", type=float, default=0.1)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--out", type=Path, help="write the fp32 checkpoint here")
    parser.add_argument(
        "--benchmark",
        action="store_true",
        help="measure and print dataloader throughput, then exit",
    )
    parser.add_argument("--benchmark-seconds", type=float, default=3.0)
    args = parser.parse_args(argv)

    data = PackedData(args.data)
    print(f"loaded {len(data):,} samples from {args.data}")

    if args.benchmark:
        benchmark_dataloader(data, args.batch_size, args.benchmark_seconds)
        return 0

    config = _build_config(args)
    model, history = train(
        data,
        config,
        epochs=args.epochs,
        batch_size=args.batch_size,
        lr=args.lr,
        lam=args.lam,
        val_fraction=args.val_fraction,
        seed=args.seed,
        device=args.device,
    )

    if args.out is not None:
        save_checkpoint(args.out, model, history)
        print(f"wrote checkpoint to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
